//! Private bounded IPC. This process cannot declare validation or publication
//! readiness. Its owner supplies a frozen reviewed installation and deadline.
use candle::{DType, Device, Tensor};
use decapod_laya_infer::{
    engine::{Engine, Frontend},
    laya_assets,
    protocol::*,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::{self, Read},
    path::Path,
    time::Instant,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn invalid(message: &str) -> io::Error {
    io::Error::other(message.to_owned())
}

fn open(path: &Path) -> Result<File> {
    if !path.is_absolute() || path.canonicalize()? != path {
        return Err(invalid("laya_noncanonical_path").into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(invalid("laya_not_regular_file").into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let uid = unsafe { libc::geteuid() };
        if (metadata.uid() != uid && metadata.uid() != 0)
            || metadata.nlink() != 1
            || metadata.mode() & 0o022 != 0
        {
            return Err(invalid("laya_unsafe_file_custody").into());
        }
    }
    Ok(file)
}

fn read(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    open(path)?.take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(invalid("laya_metadata_overflow").into());
    }
    Ok(bytes)
}

fn verify(request: &Request) -> Result<Manifest> {
    if request.schema != WORKER_SCHEMA
        || request.rubric != RUBRIC
        || request.state.is_empty()
        || request.state.len() > 32 * 1024
        || request.input_digest != laya_assets::hash(request.state.as_bytes())
        || request.request_id.len() != 71
        || !request.request_id.starts_with("sha256:")
    {
        return Err(invalid("laya_request_binding_invalid").into());
    }
    let manifest: Manifest =
        serde_json::from_slice(&read(&request.installation.join("manifest.json"), 16384)?)?;
    if !laya_assets::compatible(&manifest)
        || laya_assets::manifest_digest(&manifest)? != request.manifest_digest
    {
        return Err(invalid("laya_manifest_unreviewed_or_changed").into());
    }
    for asset in manifest
        .assets
        .iter()
        .filter(|asset| asset.backends.contains(&request.backend))
    {
        let mut file = open(&request.installation.join(&asset.path))?;
        if file.metadata()?.len() != asset.bytes {
            return Err(invalid("laya_asset_size_mismatch").into());
        }
        let mut hasher = Sha256::new();
        let mut buffer = [0; 65536];
        let mut count = 0;
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > asset.bytes {
                return Err(invalid("laya_asset_grew").into());
            }
            hasher.update(&buffer[..n]);
        }
        if count != asset.bytes || format!("{:x}", hasher.finalize()) != asset.sha256 {
            return Err(invalid("laya_asset_integrity_mismatch").into());
        }
    }
    Ok(manifest)
}

fn backend(name: &str) -> Result<Backend> {
    match name {
        "cpu" => Ok(Backend::Cpu),
        "metal" => Ok(Backend::Metal),
        "cuda" => Ok(Backend::Cuda),
        _ => Err(invalid("laya_backend_unsupported").into()),
    }
}

fn device(backend: Backend) -> candle::Result<Device> {
    match backend {
        Backend::Cpu => Ok(Device::Cpu),
        Backend::Metal => decapod_laya_infer::device(true),
        Backend::Cuda => Device::new_cuda(0),
    }
}

fn unavailable_kind(backend: Backend) -> ErrorKind {
    if (backend == Backend::Cuda && !cfg!(feature = "cuda"))
        || (backend == Backend::Metal && !cfg!(all(target_os = "macos", target_arch = "aarch64")))
    {
        ErrorKind::BackendUnsupported
    } else {
        ErrorKind::HardwareUnavailable
    }
}

fn peak_rss() -> Option<u64> {
    #[cfg(unix)]
    {
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
            return None;
        }
        let bytes = u64::try_from(usage.ru_maxrss).ok()?;
        Some(if cfg!(target_os = "macos") {
            bytes
        } else {
            bytes.checked_mul(1024)?
        })
    }
    #[cfg(not(unix))]
    {
        None
    }
}

fn probe(backend: Backend) -> BackendCheck {
    let start = Instant::now();
    let mut error_kind = None;
    let result = (|| -> Result<()> {
        let device = device(backend).map_err(|error| {
            error_kind = Some(unavailable_kind(backend));
            error
        })?;
        // Actual dispatch, synchronization and numerical readback. Enumeration
        // alone is not an execution probe. No model/state is loaded here.
        let x = Tensor::arange(0_u32, 257_u32, &device)?.to_dtype(DType::F32)?;
        let result = (&x * 2.0)?.affine(1.0, 1.0)?;
        device.synchronize()?;
        let values = result.to_vec1::<f32>()?;
        if values.len() != 257
            || values
                .iter()
                .enumerate()
                .any(|(i, v)| *v != (i * 2 + 1) as f32)
        {
            return Err(invalid("laya_dispatch_readback_mismatch").into());
        }
        Ok(())
    })();
    BackendCheck {
        schema: PROBE_SCHEMA.into(),
        backend,
        available: result.is_ok(),
        synchronized_ms: start.elapsed().as_secs_f64() * 1000.0,
        error_kind: if result.is_err() {
            Some(error_kind.unwrap_or(ErrorKind::InferenceFailed))
        } else {
            None
        },
        error: result
            .err()
            .map(|error| error.to_string().chars().take(512).collect()),
    }
}

fn execute(request: Request) -> Response {
    let start = Instant::now();
    let mut model_load_ms = 0.0;
    let mut integrity_ms = 0.0;
    let mut response = Response {
        schema: WORKER_SCHEMA.into(),
        rubric: RUBRIC.into(),
        request_id: request.request_id.clone(),
        input_digest: request.input_digest.clone(),
        manifest_digest: request.manifest_digest.clone(),
        publication_ready: false,
        judgment: None,
        error_kind: None,
        error: None,
        metrics: None,
    };
    // Hardware selection happens before any model input is evaluated. Only
    // this typed unavailability may permit a parent-owned fresh CPU attempt.
    if let Err(error) = device(request.backend) {
        response.error_kind = Some(unavailable_kind(request.backend));
        response.error = Some(error.to_string().chars().take(512).collect());
        return response;
    }
    let result = (|| -> Result<Judgment> {
        let integrity = Instant::now();
        verify(&request)?;
        integrity_ms += integrity.elapsed().as_secs_f64() * 1000.0;
        let frontend = Frontend::open(&request.installation)?;
        let prepared = frontend.prepare(&request.state)?;
        let load = Instant::now();
        let engine = Engine::open(&request.installation, request.backend)?;
        model_load_ms = load.elapsed().as_secs_f64() * 1000.0;
        let answer = engine.predict(&prepared)?;
        let integrity = Instant::now();
        verify(&request)?;
        integrity_ms += integrity.elapsed().as_secs_f64() * 1000.0;
        Ok(answer)
    })();
    match result {
        Ok(answer) => response.judgment = Some(answer),
        Err(error) => {
            response.error_kind = Some(ErrorKind::InferenceFailed);
            response.error = Some(error.to_string().chars().take(512).collect());
        }
    }
    response.metrics = Some(Metrics {
        wall_ms: start.elapsed().as_secs_f64() * 1000.0,
        model_load_ms,
        integrity_ms,
        process_peak_rss_bytes: peak_rss(),
    });
    response
}

fn main() -> Result<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(
            invalid("use --request <private-json> or --backend-check <cpu|metal|cuda>").into(),
        );
    }
    if args[0] == "--backend-check" {
        println!(
            "{}",
            serde_json::to_string(&probe(backend(
                args[1].to_str().ok_or_else(|| invalid("invalid_backend"))?
            )?))?
        );
    } else if args[0] == "--request" {
        let request = serde_json::from_slice(&read(Path::new(&args[1]), 48 * 1024)?)?;
        println!("{}", serde_json::to_string(&execute(request))?);
    } else {
        return Err(invalid("laya_unknown_argument").into());
    }
    Ok(())
}
