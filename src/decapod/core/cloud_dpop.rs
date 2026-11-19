//! Device-bound Ed25519 keypair custody and RFC 9449 DPoP proof generation (#1778, #1033).
//!
//! Private key material is bound to an origin + canonical repository scope,
//! persisted via OS keychain when enabled or a strict mode-0600 private-store
//! fallback outside any repository checkout, and never serialized to tracked
//! config files, environment variables, command arguments, URLs, or logs.

use crate::core::error::{CloudAuthDiagnostic, CloudAuthStatus, DecapodError};
use crate::core::fs_permissions::ensure_private_dir;
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

const DEVICE_KEY_SCHEMA_VERSION: u32 = 1;
const KEYSTORE_SERVICE: &str = "com.decapod.cloud.device-key";
pub const MAX_DPOP_CLOCK_SKEW_SECONDS: i64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKeyStorageBackend {
    OsKeychain,
    PrivateFile0600,
}

impl DeviceKeyStorageBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OsKeychain => "os_keychain",
            Self::PrivateFile0600 => "private_file_0600",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicEd25519Jwk {
    pub crv: String,
    pub kty: String,
    pub x: String,
}

impl PublicEd25519Jwk {
    pub fn from_public_bytes(pub_bytes: &[u8]) -> Result<Self, DecapodError> {
        if pub_bytes.len() != 32 {
            return Err(DecapodError::ValidationError(
                "Ed25519 public key must be 32 bytes".to_string(),
            ));
        }
        Ok(Self {
            crv: "Ed25519".to_string(),
            kty: "OKP".to_string(),
            x: base64url_encode(pub_bytes),
        })
    }

    /// Canonical RFC 7638 JWK JSON with keys sorted lexicographically: crv, kty, x.
    pub fn canonical_rfc7638_json(&self) -> String {
        format!(
            "{{\"crv\":\"{}\",\"kty\":\"{}\",\"x\":\"{}\"}}",
            self.crv, self.kty, self.x
        )
    }

    /// RFC 7638 SHA-256 JWK thumbprint encoded with base64url (no padding).
    pub fn thumbprint_jkt(&self) -> String {
        let digest = Sha256::digest(self.canonical_rfc7638_json().as_bytes());
        base64url_encode(&digest)
    }
}

pub struct DeviceKeyMaterial {
    pkcs8_bytes: Vec<u8>,
    pub public_jwk: PublicEd25519Jwk,
    pub jkt: String,
    pub backend: DeviceKeyStorageBackend,
    pub created_at: i64,
}

impl Drop for DeviceKeyMaterial {
    fn drop(&mut self) {
        zeroize_bytes(&mut self.pkcs8_bytes);
    }
}

impl fmt::Debug for DeviceKeyMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceKeyMaterial")
            .field("pkcs8_bytes", &"[REDACTED]")
            .field("jkt", &self.jkt)
            .field("backend", &self.backend)
            .field("created_at", &self.created_at)
            .finish()
    }
}

impl DeviceKeyMaterial {
    pub fn generate(backend: DeviceKeyStorageBackend) -> Result<Self, DecapodError> {
        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).map_err(|_| {
            DecapodError::ValidationError("failed to generate device Ed25519 keypair".to_string())
        })?;
        Self::from_pkcs8(pkcs8.as_ref().to_vec(), backend, current_unix_seconds())
    }

    pub fn from_pkcs8(
        pkcs8_bytes: Vec<u8>,
        backend: DeviceKeyStorageBackend,
        created_at: i64,
    ) -> Result<Self, DecapodError> {
        let pair = Ed25519KeyPair::from_pkcs8(&pkcs8_bytes).map_err(|_| {
            DecapodError::CloudAuth(CloudAuthDiagnostic::new(
                CloudAuthStatus::Unauthorized,
                "Stored device signing key could not be decoded",
                "Re-enroll this device using `decapod cloud login --restart`.",
            ))
        })?;
        let public_jwk = PublicEd25519Jwk::from_public_bytes(pair.public_key().as_ref())?;
        let jkt = public_jwk.thumbprint_jkt();
        Ok(Self {
            pkcs8_bytes,
            public_jwk,
            jkt,
            backend,
            created_at,
        })
    }

    /// Generate a fresh RFC 9449 DPoP proof JWT bound to HTTP method, canonical
    /// target URI (`htm`/`htu`), optional access token (`ath` computed from raw
    /// token), server `nonce`, and `aud`.
    pub fn sign_dpop_proof(
        &self,
        method: &str,
        url: &str,
        access_token: Option<&str>,
        nonce: Option<&str>,
        audience: Option<&str>,
        now_secs: Option<i64>,
    ) -> Result<String, DecapodError> {
        let ath = access_token.map(dpop_access_token_hash).transpose()?;
        self.sign_dpop_proof_with_ath(method, url, ath.as_deref(), nonce, audience, now_secs)
    }

    /// Generate a fresh RFC 9449 DPoP proof JWT using a precomputed base64url
    /// SHA-256 `ath` digest (such as `dactyl_db::RequestProofInput::access_token_sha256`).
    pub fn sign_dpop_proof_with_ath(
        &self,
        method: &str,
        url: &str,
        ath: Option<&str>,
        nonce: Option<&str>,
        audience: Option<&str>,
        now_secs: Option<i64>,
    ) -> Result<String, DecapodError> {
        let pair = Ed25519KeyPair::from_pkcs8(&self.pkcs8_bytes).map_err(|_| {
            DecapodError::ValidationError("device signing key is unavailable or corrupt".into())
        })?;
        let htm = normalize_http_method(method)?;
        let htu = canonical_dpop_htu(url)?;
        let iat = now_secs.unwrap_or_else(current_unix_seconds);
        let jti = random_jti()?;

        let header = serde_json::json!({
            "typ": "dpop+jwt",
            "alg": "EdDSA",
            "jwk": self.public_jwk,
        });

        let mut claims = serde_json::Map::new();
        claims.insert("jti".to_string(), serde_json::Value::String(jti));
        claims.insert("htm".to_string(), serde_json::Value::String(htm));
        claims.insert("htu".to_string(), serde_json::Value::String(htu));
        claims.insert("iat".to_string(), serde_json::Value::Number(iat.into()));
        if let Some(ath) = ath {
            let trimmed = ath.trim();
            if !trimmed.is_empty() {
                claims.insert(
                    "ath".to_string(),
                    serde_json::Value::String(trimmed.to_string()),
                );
            }
        }
        if let Some(nonce) = nonce {
            let trimmed = nonce.trim();
            if !trimmed.is_empty() {
                claims.insert(
                    "nonce".to_string(),
                    serde_json::Value::String(trimmed.to_string()),
                );
            }
        }
        if let Some(aud) = audience {
            let trimmed = aud.trim();
            if !trimmed.is_empty() {
                claims.insert(
                    "aud".to_string(),
                    serde_json::Value::String(trimmed.to_string()),
                );
            }
        }

        let header_bytes = serde_json::to_vec(&header).map_err(|e| {
            DecapodError::ValidationError(format!("failed to encode DPoP header: {e}"))
        })?;
        let claims_bytes = serde_json::to_vec(&serde_json::Value::Object(claims)).map_err(|e| {
            DecapodError::ValidationError(format!("failed to encode DPoP claims: {e}"))
        })?;

        let signing_input = format!(
            "{}.{}",
            base64url_encode(&header_bytes),
            base64url_encode(&claims_bytes)
        );
        let sig = pair.sign(signing_input.as_bytes());
        Ok(format!(
            "{signing_input}.{}",
            base64url_encode(sig.as_ref())
        ))
    }
}

#[derive(Serialize, Deserialize)]
struct PersistedDeviceKeyRecord {
    schema_version: u32,
    origin: String,
    repository: String,
    jkt: String,
    pkcs8_b64url: String,
    created_at: i64,
}

impl Drop for PersistedDeviceKeyRecord {
    fn drop(&mut self) {
        zeroize_string(&mut self.pkcs8_b64url);
    }
}

#[derive(Debug, Clone)]
pub struct DeviceKeyStore {
    origin: String,
    repository: String,
    scope_key: String,
    file_path: PathBuf,
    allow_os_keychain: bool,
}

impl DeviceKeyStore {
    pub fn for_project(
        origin: &str,
        repository: &str,
        project_root: &Path,
    ) -> Result<Self, DecapodError> {
        let base_dir = device_keys_root_dir(project_root)?;
        let mut store = Self::with_root_dir(origin, repository, project_root, &base_dir)?;
        store.allow_os_keychain = should_use_os_keychain(&base_dir);
        Ok(store)
    }

    pub fn with_root_dir(
        origin: &str,
        repository: &str,
        project_root: &Path,
        base_dir: &Path,
    ) -> Result<Self, DecapodError> {
        let canonical_base = canonicalize_with_nonexistent_tail(base_dir);
        if let Ok(canonical_project) = project_root.canonicalize()
            && canonical_base.starts_with(&canonical_project)
        {
            return Err(DecapodError::ValidationError(
                "DEVICE_KEY_PATH_FORBIDDEN: device signing keys must reside outside repository directories"
                    .to_string(),
            ));
        }
        let scope_key = scope_hash(origin, repository);
        let file_path = base_dir.join(format!("{scope_key}.pkcs8.json"));
        Ok(Self {
            origin: origin.trim_end_matches('/').to_string(),
            repository: repository.trim().to_string(),
            scope_key,
            file_path,
            allow_os_keychain: false,
        })
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    pub fn load(&self) -> Result<Option<DeviceKeyMaterial>, DecapodError> {
        if self.allow_os_keychain
            && let Some(raw) = keychain_load(&self.scope_key)?
        {
            let record: PersistedDeviceKeyRecord = serde_json::from_str(&raw).map_err(|_| {
                DecapodError::ValidationError("DEVICE_KEY_CORRUPT: keychain record invalid".into())
            })?;
            return self
                .decode_record(record, DeviceKeyStorageBackend::OsKeychain)
                .map(Some);
        }
        self.load_from_private_file()
    }

    pub fn load_or_create(&self) -> Result<DeviceKeyMaterial, DecapodError> {
        if let Some(existing) = self.load()? {
            return Ok(existing);
        }
        let target_backend = if self.allow_os_keychain {
            DeviceKeyStorageBackend::OsKeychain
        } else {
            DeviceKeyStorageBackend::PrivateFile0600
        };
        let key = DeviceKeyMaterial::generate(target_backend)?;
        self.save(&key)
    }

    pub fn rotate(&self) -> Result<DeviceKeyMaterial, DecapodError> {
        self.delete()?;
        let target_backend = if self.allow_os_keychain {
            DeviceKeyStorageBackend::OsKeychain
        } else {
            DeviceKeyStorageBackend::PrivateFile0600
        };
        let key = DeviceKeyMaterial::generate(target_backend)?;
        self.save(&key)
    }

    pub fn save(&self, key: &DeviceKeyMaterial) -> Result<DeviceKeyMaterial, DecapodError> {
        let mut record = PersistedDeviceKeyRecord {
            schema_version: DEVICE_KEY_SCHEMA_VERSION,
            origin: self.origin.clone(),
            repository: self.repository.clone(),
            jkt: key.jkt.clone(),
            pkcs8_b64url: base64url_encode(&key.pkcs8_bytes),
            created_at: key.created_at,
        };
        let json = serde_json::to_string(&record).map_err(|e| {
            DecapodError::ValidationError(format!("failed to serialize device key: {e}"))
        })?;
        zeroize_string(&mut record.pkcs8_b64url);

        if self.allow_os_keychain && keychain_store(&self.scope_key, &json).is_ok() {
            return DeviceKeyMaterial::from_pkcs8(
                key.pkcs8_bytes.clone(),
                DeviceKeyStorageBackend::OsKeychain,
                key.created_at,
            );
        }
        self.save_to_private_file(&json)?;
        DeviceKeyMaterial::from_pkcs8(
            key.pkcs8_bytes.clone(),
            DeviceKeyStorageBackend::PrivateFile0600,
            key.created_at,
        )
    }

    pub fn delete(&self) -> Result<(), DecapodError> {
        if self.allow_os_keychain {
            let _ = keychain_delete(&self.scope_key);
        }
        match fs::symlink_metadata(&self.file_path) {
            Ok(meta) => {
                if meta.file_type().is_symlink() {
                    return Err(DecapodError::ValidationError(
                        "DEVICE_KEY_SYMLINK_REJECTED: refusing to follow symlink".into(),
                    ));
                }
                fs::remove_file(&self.file_path).map_err(DecapodError::IoError)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(DecapodError::IoError(err)),
        }
    }

    fn decode_record(
        &self,
        record: PersistedDeviceKeyRecord,
        backend: DeviceKeyStorageBackend,
    ) -> Result<DeviceKeyMaterial, DecapodError> {
        if record.schema_version != DEVICE_KEY_SCHEMA_VERSION
            || record.origin != self.origin
            || record.repository != self.repository
        {
            return Err(DecapodError::ValidationError(
                "DEVICE_KEY_SCOPE_MISMATCH: stored device key belongs to another origin or repository"
                    .to_string(),
            ));
        }
        let pkcs8 = base64url_decode(&record.pkcs8_b64url)?;
        let mat = DeviceKeyMaterial::from_pkcs8(pkcs8, backend, record.created_at)?;
        if mat.jkt != record.jkt {
            return Err(DecapodError::ValidationError(
                "DEVICE_KEY_CORRUPT: JWK thumbprint mismatch".to_string(),
            ));
        }
        Ok(mat)
    }

    fn load_from_private_file(&self) -> Result<Option<DeviceKeyMaterial>, DecapodError> {
        let meta = match fs::symlink_metadata(&self.file_path) {
            Ok(meta) => meta,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(DecapodError::IoError(err)),
        };
        if meta.file_type().is_symlink() {
            return Err(DecapodError::ValidationError(
                "DEVICE_KEY_SYMLINK_REJECTED: device key path must not be a symlink".into(),
            ));
        }
        #[cfg(unix)]
        {
            let mode = meta.permissions().mode() & 0o777;
            if (mode & 0o077) != 0 {
                return Err(DecapodError::ValidationError(format!(
                    "DEVICE_KEY_INSECURE_PERMISSIONS: private key store has mode {:03o}; expected 0600",
                    mode
                )));
            }
        }
        let raw = fs::read_to_string(&self.file_path).map_err(DecapodError::IoError)?;
        let record: PersistedDeviceKeyRecord = serde_json::from_str(&raw).map_err(|_| {
            DecapodError::ValidationError("DEVICE_KEY_CORRUPT: invalid key store payload".into())
        })?;
        self.decode_record(record, DeviceKeyStorageBackend::PrivateFile0600)
            .map(Some)
    }

    fn save_to_private_file(&self, json: &str) -> Result<(), DecapodError> {
        let parent = self
            .file_path
            .parent()
            .ok_or_else(|| DecapodError::PathError("invalid device key path".into()))?;
        ensure_private_dir(parent).map_err(DecapodError::IoError)?;
        if let Ok(meta) = fs::symlink_metadata(&self.file_path) {
            if meta.file_type().is_symlink() {
                return Err(DecapodError::ValidationError(
                    "DEVICE_KEY_SYMLINK_REJECTED: refusing to overwrite symlink".into(),
                ));
            }
            fs::remove_file(&self.file_path).map_err(DecapodError::IoError)?;
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
            options.custom_flags(libc::O_NOFOLLOW);
        }
        let mut file = options
            .open(&self.file_path)
            .map_err(DecapodError::IoError)?;
        file.write_all(json.as_bytes())
            .map_err(DecapodError::IoError)?;
        file.sync_all().map_err(DecapodError::IoError)?;
        #[cfg(unix)]
        fs::set_permissions(&self.file_path, fs::Permissions::from_mode(0o600))
            .map_err(DecapodError::IoError)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedDpopClaims {
    pub jkt: String,
    pub jti: String,
    pub htm: String,
    pub htu: String,
    pub iat: i64,
    pub ath: Option<String>,
    pub nonce: Option<String>,
    pub aud: Option<String>,
}

/// Offline RFC 9449 EdDSA DPoP verifier used for client conformance tests and
/// bounded host-side verification checks.
pub fn verify_dpop_proof(
    jwt: &str,
    expected_method: &str,
    expected_url: &str,
    expected_access_token: Option<&str>,
    expected_nonce: Option<&str>,
    expected_aud: Option<&str>,
    now_secs: i64,
) -> Result<VerifiedDpopClaims, DecapodError> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        return Err(DecapodError::ValidationError(
            "invalid DPoP JWT format".into(),
        ));
    }
    let header_bytes = base64url_decode(parts[0])?;
    let claims_bytes = base64url_decode(parts[1])?;
    let sig_bytes = base64url_decode(parts[2])?;

    let header: serde_json::Value = serde_json::from_slice(&header_bytes)
        .map_err(|_| DecapodError::ValidationError("invalid DPoP header JSON".into()))?;
    if header.get("typ").and_then(|v| v.as_str()) != Some("dpop+jwt") {
        return Err(DecapodError::ValidationError("invalid DPoP typ".into()));
    }
    if header.get("alg").and_then(|v| v.as_str()) != Some("EdDSA") {
        return Err(DecapodError::ValidationError("unsupported DPoP alg".into()));
    }
    let jwk_obj = header
        .get("jwk")
        .and_then(|v| v.as_object())
        .ok_or_else(|| DecapodError::ValidationError("missing DPoP jwk".into()))?;
    for forbidden in ["d", "p", "q", "dp", "dq", "qi", "oth", "k"] {
        if jwk_obj.contains_key(forbidden) {
            return Err(DecapodError::ValidationError(
                "DPoP jwk must never contain private key parameters".into(),
            ));
        }
    }
    let jwk: PublicEd25519Jwk = serde_json::from_value(serde_json::Value::Object(jwk_obj.clone()))
        .map_err(|_| DecapodError::ValidationError("invalid DPoP OKP jwk".into()))?;
    if jwk.kty != "OKP" || jwk.crv != "Ed25519" {
        return Err(DecapodError::ValidationError(
            "invalid DPoP jwk curve/type".into(),
        ));
    }
    let pub_bytes = base64url_decode(&jwk.x)?;
    if pub_bytes.len() != 32 {
        return Err(DecapodError::ValidationError(
            "invalid Ed25519 x coordinate".into(),
        ));
    }
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    UnparsedPublicKey::new(&ED25519, &pub_bytes)
        .verify(signing_input.as_bytes(), &sig_bytes)
        .map_err(|_| DecapodError::ValidationError("DPoP signature verification failed".into()))?;

    let claims: serde_json::Value = serde_json::from_slice(&claims_bytes)
        .map_err(|_| DecapodError::ValidationError("invalid DPoP claims JSON".into()))?;
    let jti = claims
        .get("jti")
        .and_then(|v| v.as_str())
        .filter(|s| (16..=128).contains(&s.len()))
        .ok_or_else(|| DecapodError::ValidationError("invalid DPoP jti".into()))?
        .to_string();
    let htm = claims
        .get("htm")
        .and_then(|v| v.as_str())
        .ok_or_else(|| DecapodError::ValidationError("missing DPoP htm".into()))?
        .to_string();
    if htm != normalize_http_method(expected_method)? {
        return Err(DecapodError::ValidationError("DPoP htm mismatch".into()));
    }
    let htu = claims
        .get("htu")
        .and_then(|v| v.as_str())
        .ok_or_else(|| DecapodError::ValidationError("missing DPoP htu".into()))?
        .to_string();
    if htu != canonical_dpop_htu(expected_url)? {
        return Err(DecapodError::ValidationError("DPoP htu mismatch".into()));
    }
    let iat = claims
        .get("iat")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| DecapodError::ValidationError("missing DPoP iat".into()))?;
    if (now_secs - iat).abs() > MAX_DPOP_CLOCK_SKEW_SECONDS {
        return Err(DecapodError::ValidationError(
            "DPoP iat outside skew window".into(),
        ));
    }
    let ath = claims
        .get("ath")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if let Some(token) = expected_access_token {
        let expected_ath = dpop_access_token_hash(token)?;
        if ath.as_deref() != Some(expected_ath.as_str()) {
            return Err(DecapodError::ValidationError("DPoP ath mismatch".into()));
        }
    }
    let nonce = claims
        .get("nonce")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if let Some(exp_nonce) = expected_nonce
        && nonce.as_deref() != Some(exp_nonce)
    {
        return Err(DecapodError::ValidationError("DPoP nonce mismatch".into()));
    }
    let aud = claims
        .get("aud")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if let Some(exp_aud) = expected_aud
        && aud.as_deref() != Some(exp_aud)
    {
        return Err(DecapodError::ValidationError("DPoP aud mismatch".into()));
    }

    Ok(VerifiedDpopClaims {
        jkt: jwk.thumbprint_jkt(),
        jti,
        htm,
        htu,
        iat,
        ath,
        nonce,
        aud,
    })
}

pub fn dpop_access_token_hash(access_token: &str) -> Result<String, DecapodError> {
    if access_token.trim().is_empty() {
        return Err(DecapodError::ValidationError(
            "cannot compute DPoP ath from empty token".into(),
        ));
    }
    let digest = Sha256::digest(access_token.as_bytes());
    Ok(base64url_encode(&digest))
}

pub fn canonical_dpop_htu(url: &str) -> Result<String, DecapodError> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(DecapodError::ValidationError(
            "DPoP URL must not contain whitespace or control characters".into(),
        ));
    }
    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let Some((scheme, rest)) = without_query.split_once("://") else {
        return Err(DecapodError::ValidationError(
            "DPoP URL must include an HTTP(S) scheme".into(),
        ));
    };
    let scheme_lower = scheme.to_ascii_lowercase();
    if scheme_lower != "https" && scheme_lower != "http" {
        return Err(DecapodError::ValidationError(
            "DPoP URL scheme must be http or https".into(),
        ));
    }
    let (authority, path_suffix) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, ""),
    };
    if authority.is_empty() || authority.contains('@') {
        return Err(DecapodError::ValidationError(
            "DPoP URL authority must be valid and omit userinfo".into(),
        ));
    }
    Ok(format!(
        "{scheme_lower}://{}{}",
        authority.to_ascii_lowercase(),
        if path_suffix.is_empty() {
            "/"
        } else {
            path_suffix
        }
    ))
}

fn normalize_http_method(method: &str) -> Result<String, DecapodError> {
    let trimmed = method.trim().to_ascii_uppercase();
    if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(DecapodError::ValidationError(
            "DPoP HTTP method must be ASCII letters".into(),
        ));
    }
    Ok(trimmed)
}

fn random_jti() -> Result<String, DecapodError> {
    let mut bytes = [0u8; 24];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| DecapodError::ValidationError("failed generating random DPoP jti".into()))?;
    Ok(base64url_encode(&bytes))
}

fn scope_hash(origin: &str, repository: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(origin.trim_end_matches('/').as_bytes());
    hasher.update([0u8]);
    hasher.update(repository.trim().as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(32);
    for byte in &digest[..16] {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn device_keys_root_dir(project_root: &Path) -> Result<PathBuf, DecapodError> {
    let dir = if let Some(xdg) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        PathBuf::from(xdg).join("decapod").join("device-keys")
    } else if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("decapod")
            .join("device-keys")
    } else {
        return Err(DecapodError::Config(
            "Unable to locate XDG_DATA_HOME or HOME for device key custody".into(),
        ));
    };
    let canonical_project = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    if dir.starts_with(&canonical_project) {
        return Err(DecapodError::ValidationError(
            "DEVICE_KEY_PATH_FORBIDDEN: device key store must reside outside repository".into(),
        ));
    }
    Ok(dir)
}

fn should_use_os_keychain(base_dir: &Path) -> bool {
    match std::env::var("DECAPOD_CLOUD_KEYSTORE").as_deref() {
        Ok("file") => return false,
        Ok("keychain") => return cfg!(target_os = "macos"),
        _ => {}
    }
    if cfg!(test) || std::env::var_os("XDG_DATA_HOME").is_some() {
        let _ = base_dir;
        return false;
    }
    cfg!(target_os = "macos")
}

#[cfg(target_os = "macos")]
fn keychain_load(scope_key: &str) -> Result<Option<String>, DecapodError> {
    let output = std::process::Command::new("security")
        .args([
            "find-generic-password",
            "-s",
            KEYSTORE_SERVICE,
            "-a",
            scope_key,
            "-w",
        ])
        .output()
        .map_err(DecapodError::IoError)?;
    if output.status.success() {
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if value.is_empty() {
            return Ok(None);
        }
        return Ok(Some(value));
    }
    Ok(None)
}

#[cfg(not(target_os = "macos"))]
fn keychain_load(_scope_key: &str) -> Result<Option<String>, DecapodError> {
    Ok(None)
}

#[cfg(target_os = "macos")]
fn keychain_store(scope_key: &str, payload: &str) -> Result<(), DecapodError> {
    let status = std::process::Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-s",
            KEYSTORE_SERVICE,
            "-a",
            scope_key,
            "-w",
            payload,
        ])
        .status()
        .map_err(DecapodError::IoError)?;
    if status.success() {
        Ok(())
    } else {
        Err(DecapodError::ValidationError(
            "OS keychain write failed; falling back to mode-0600 private store".into(),
        ))
    }
}

#[cfg(not(target_os = "macos"))]
fn keychain_store(_scope_key: &str, _payload: &str) -> Result<(), DecapodError> {
    Err(DecapodError::ValidationError(
        "OS keychain unsupported on this platform".into(),
    ))
}

#[cfg(target_os = "macos")]
fn keychain_delete(scope_key: &str) -> Result<(), DecapodError> {
    let _ = std::process::Command::new("security")
        .args([
            "delete-generic-password",
            "-s",
            KEYSTORE_SERVICE,
            "-a",
            scope_key,
        ])
        .status();
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn keychain_delete(_scope_key: &str) -> Result<(), DecapodError> {
    Ok(())
}

fn current_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn canonicalize_with_nonexistent_tail(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut tail = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name().map(|s| s.to_os_string()) else {
            return path.to_path_buf();
        };
        tail.push(name);
        let Some(parent) = existing.parent() else {
            return path.to_path_buf();
        };
        existing = parent.to_path_buf();
    }
    let mut out = existing.canonicalize().unwrap_or(existing);
    for comp in tail.into_iter().rev() {
        out.push(comp);
    }
    out
}

fn zeroize_bytes(buf: &mut [u8]) {
    for b in buf.iter_mut() {
        unsafe { std::ptr::write_volatile(b, 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}

fn zeroize_string(s: &mut String) {
    unsafe { zeroize_bytes(s.as_bytes_mut()) };
    s.clear();
}

const B64URL_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn base64url_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 {
            chunk[1] as usize
        } else {
            0
        };
        let b2 = if chunk.len() > 2 {
            chunk[2] as usize
        } else {
            0
        };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64URL_TABLE[(triple >> 18) & 0x3f] as char);
        out.push(B64URL_TABLE[(triple >> 12) & 0x3f] as char);
        if chunk.len() > 1 {
            out.push(B64URL_TABLE[(triple >> 6) & 0x3f] as char);
        }
        if chunk.len() > 2 {
            out.push(B64URL_TABLE[triple & 0x3f] as char);
        }
    }
    out
}

pub fn base64url_decode(input: &str) -> Result<Vec<u8>, DecapodError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 == 1 {
        return Err(DecapodError::ValidationError(
            "invalid base64url length".into(),
        ));
    }
    let mut out = Vec::with_capacity((bytes.len() * 3) / 4);
    for chunk in bytes.chunks(4) {
        let vals: Vec<u8> = chunk
            .iter()
            .map(|&b| match b {
                b'A'..=b'Z' => Ok(b - b'A'),
                b'a'..=b'z' => Ok(b - b'a' + 26),
                b'0'..=b'9' => Ok(b - b'0' + 52),
                b'-' => Ok(62),
                b'_' => Ok(63),
                _ => Err(DecapodError::ValidationError(
                    "invalid base64url character".into(),
                )),
            })
            .collect::<Result<Vec<_>, _>>()?;
        out.push((vals[0] << 2) | (vals[1] >> 4));
        if vals.len() >= 3 {
            out.push((vals[1] << 4) | (vals[2] >> 2));
        }
        if vals.len() == 4 {
            out.push((vals[2] << 6) | vals[3]);
        }
    }
    Ok(out)
}

/// Synchronous `dactyl_db::RequestProofSigner` backed by a locally custodied
/// Ed25519 device key (`#1778` / `dactyl-db 0.18.0`). Tracks bounded server
/// `DPoP-Nonce` updates observed on responses/challenges.
pub struct DeviceDpopSigner {
    key: std::sync::Arc<DeviceKeyMaterial>,
    observed_nonce: std::sync::Mutex<Option<String>>,
}

impl DeviceDpopSigner {
    pub fn new(key: DeviceKeyMaterial) -> Self {
        Self {
            key: std::sync::Arc::new(key),
            observed_nonce: std::sync::Mutex::new(None),
        }
    }

    pub fn jkt(&self) -> &str {
        &self.key.jkt
    }
}

impl dactyl_db::RequestProofSigner for DeviceDpopSigner {
    fn sign_request(
        &self,
        input: &dactyl_db::RequestProofInput<'_>,
    ) -> Result<dactyl_db::RequestProof, dactyl_db::DactylError> {
        let cached_nonce = self
            .observed_nonce
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        let nonce = input.nonce().or(cached_nonce.as_deref());
        let jwt = self
            .key
            .sign_dpop_proof_with_ath(
                input.method(),
                input.canonical_url(),
                input.access_token_sha256(),
                nonce,
                None,
                None,
            )
            .map_err(|err| dactyl_db::DactylError::Adapter {
                kind: dactyl_db::AdapterErrorKind::Authentication,
                code: Some("invalid_dpop_proof".to_string()),
                message: err.to_string(),
            })?;
        dactyl_db::RequestProof::dpop(jwt)
    }

    fn observe_nonce(&self, _canonical_url: &str, nonce: &str) {
        let trimmed = nonce.trim();
        if !trimmed.is_empty()
            && trimmed.len() <= 256
            && let Ok(mut guard) = self.observed_nonce.lock()
        {
            *guard = Some(trimmed.to_string());
        }
    }
}

/// Container-workspace `dactyl_db::RequestProofSigner` (`#1066`): forwards
/// signing requests over the bounded host-mounted Unix domain socket so private
/// keys never enter the container filesystem or environment.
pub struct ContainerBrokerDpopSigner {
    socket_path: PathBuf,
    repository: String,
    agent_id: String,
    observed_nonce: std::sync::Mutex<Option<String>>,
}

impl ContainerBrokerDpopSigner {
    pub fn new(socket_path: PathBuf, repository: String, agent_id: String) -> Self {
        Self {
            socket_path,
            repository,
            agent_id,
            observed_nonce: std::sync::Mutex::new(None),
        }
    }
}

impl dactyl_db::RequestProofSigner for ContainerBrokerDpopSigner {
    fn sign_request(
        &self,
        input: &dactyl_db::RequestProofInput<'_>,
    ) -> Result<dactyl_db::RequestProof, dactyl_db::DactylError> {
        let cached_nonce = self
            .observed_nonce
            .lock()
            .ok()
            .and_then(|guard| guard.clone());
        let nonce = input.nonce().map(str::to_string).or(cached_nonce);
        let req = crate::core::container_signer::ContainerSignRequest {
            capability: crate::core::container_signer::SignerCapabilityKind::PropodusCloud,
            repository: self.repository.clone(),
            agent_id: self.agent_id.clone(),
            method: input.method().to_string(),
            url: input.canonical_url().to_string(),
            nonce,
            aud: None,
            access_token_sha256: input.access_token_sha256().map(str::to_string),
        };
        let resp =
            crate::core::container_signer::request_signature_via_socket(&self.socket_path, &req)
                .map_err(|err| dactyl_db::DactylError::Adapter {
                    kind: dactyl_db::AdapterErrorKind::Authentication,
                    code: Some("container_signer_unavailable".to_string()),
                    message: err.to_string(),
                })?;
        if !resp.ok {
            return Err(dactyl_db::DactylError::Adapter {
                kind: dactyl_db::AdapterErrorKind::Authorization,
                code: Some(
                    resp.code
                        .unwrap_or_else(|| "container_signer_denied".to_string()),
                ),
                message: resp
                    .message
                    .unwrap_or_else(|| "host container signer denied request".to_string()),
            });
        }
        let proof = resp
            .dpop_proof
            .ok_or_else(|| dactyl_db::DactylError::Adapter {
                kind: dactyl_db::AdapterErrorKind::Authentication,
                code: Some("invalid_dpop_proof".to_string()),
                message: "host container signer returned no DPoP proof".to_string(),
            })?;
        dactyl_db::RequestProof::dpop(proof)
    }

    fn observe_nonce(&self, _canonical_url: &str, nonce: &str) {
        let trimmed = nonce.trim();
        if !trimmed.is_empty()
            && trimmed.len() <= 256
            && let Ok(mut guard) = self.observed_nonce.lock()
        {
            *guard = Some(trimmed.to_string());
        }
    }
}

/// Resolve the active `RequestProofSigner` for a cloud connector if running
/// inside a container signer broker workspace (`DECAPOD_CONTAINER_SIGNER_SOCKET`)
/// or when DPoP auth scheme is explicitly active with an enrolled device key.
pub fn active_request_proof_signer(
    origin: &str,
    canonical_repo: &str,
    project_root: &Path,
) -> Option<std::sync::Arc<dyn dactyl_db::RequestProofSigner>> {
    if let Ok(socket) = std::env::var("DECAPOD_CONTAINER_SIGNER_SOCKET") {
        let trimmed = socket.trim();
        if !trimmed.is_empty() {
            let repo = std::env::var("DECAPOD_CONTAINER_SIGNER_REPO")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| canonical_repo.to_string());
            let agent_id = std::env::var("DECAPOD_AGENT_ID")
                .ok()
                .filter(|v| !v.trim().is_empty())
                .unwrap_or_else(|| "container-agent".to_string());
            return Some(std::sync::Arc::new(ContainerBrokerDpopSigner::new(
                PathBuf::from(trimmed),
                repo,
                agent_id,
            )));
        }
    }
    let force_dpop = std::env::var("DECAPOD_CLOUD_AUTH_SCHEME")
        .ok()
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("dpop"));
    if !force_dpop {
        return None;
    }
    let store = DeviceKeyStore::for_project(origin, canonical_repo, project_root).ok()?;
    let key = store.load().ok().flatten()?;
    Some(std::sync::Arc::new(DeviceDpopSigner::new(key)))
}

#[cfg(test)]
#[path = "../../../tests/unit/core/cloud_dpop_tests.rs"]
mod tests;
