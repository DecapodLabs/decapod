//! Reviewed immutable Laya profile. No floating refs, model discovery or fitting
//! occurs during validation. ONNX is a community conversion, explicitly pinned
//! separately from the original checkpoint and the maintained Rust SDK.
use super::laya_protocol::{Asset, Backend, MANIFEST_SCHEMA, Manifest, SDK_REVISION};
use sha2::{Digest, Sha256};

pub const CHECKPOINT: &str = "convaiinnovations/laya";
pub const CHECKPOINT_REVISION: &str = "7b928d828b7b0e022f929d9bd2e44165aa270148";
pub const CONVERSION: &str = "desplega/laya-onnx";
pub const CONVERSION_REVISION: &str = "511bd780d5ab8592bee0a6298fc8c8ab386a268f";

pub fn baseline() -> Manifest {
    let mut assets = Vec::new();
    for (path, upstream, bytes, sha256, native) in [
        (
            "encoder-config.json",
            "multilingual/encoder/config.json",
            1938,
            "83f6916d13ef0f556ac461f28308dc2bffa7ebeadee8ec9e2db5812020ea5bb4",
            true,
        ),
        (
            "model.safetensors",
            "multilingual/model.safetensors",
            643835514,
            "9d628fd971b700382ac6f65920a86f149777b2e748e0c955fb3b19695aa8f204",
            true,
        ),
        (
            "onnx/encoder.onnx",
            "multilingual/fp32/encoder.onnx",
            1230282662,
            "4bf4feb8de3ee17e8032cf0c2fc506131d2e3c458dbe3ea82898d4d2d06fb1a6",
            false,
        ),
        (
            "onnx/head.onnx",
            "multilingual/fp32/head.onnx",
            60101236,
            "03f78ddf0bfaa86f7b24e4298070bf9ac0a03516e64e8441552ecc2db89e1b3c",
            false,
        ),
        (
            "onnx/tokenizer.json",
            "multilingual/fp32/tokenizer.json",
            34363188,
            "609d8f4c067cd3950f88594c5a802616cea245823836ef5848ee4fc40aab5b6f",
            false,
        ),
        (
            "onnx/rl_agent_config.json",
            "multilingual/fp32/rl_agent_config.json",
            472,
            "25061739243b617ad88d1219ba6f8a9c86c5881ca28df024fa2d9b3b2fcc30c6",
            false,
        ),
    ] {
        let shared = path.ends_with("tokenizer.json") || path.ends_with("rl_agent_config.json");
        assets.push(Asset {
            path: path.into(),
            repository: if native { CHECKPOINT } else { CONVERSION }.into(),
            revision: if native {
                CHECKPOINT_REVISION
            } else {
                CONVERSION_REVISION
            }
            .into(),
            upstream_path: upstream.into(),
            bytes,
            sha256: sha256.into(),
            backends: if shared {
                vec![Backend::Cpu, Backend::Metal, Backend::Cuda]
            } else if native {
                vec![Backend::Metal, Backend::Cuda]
            } else {
                vec![Backend::Cpu]
            },
        });
    }
    Manifest {
        schema: MANIFEST_SCHEMA.into(),
        profile: "laya-multilingual-fp32-1024-v1".into(),
        checkpoint: CHECKPOINT.into(),
        checkpoint_revision: CHECKPOINT_REVISION.into(),
        source_revision: SDK_REVISION.into(),
        max_tokens: 1024,
        license: "Apache-2.0".into(),
        assets,
    }
}

pub fn hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

pub fn manifest_digest(manifest: &Manifest) -> Result<String, serde_json::Error> {
    // Structs/vectors have fixed field order; no dynamic JSON object ordering.
    Ok(hash(&serde_json::to_vec(manifest)?))
}

pub fn compatible(manifest: &Manifest) -> bool {
    // An upstream update is a reviewable profile change, never a silent swap.
    manifest == &baseline()
}
