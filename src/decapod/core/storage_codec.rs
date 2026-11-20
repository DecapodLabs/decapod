//! Versioned physical compression for cold LCM summary fields.
//! No mutation of canonical event payloads or proof IDs is permitted.
use crate::core::db::ToSql;
use crate::core::error::DecapodError;
use dactyl_db::Parameter;
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

const MAGIC: &[u8; 8] = b"DCPLCM1\0";
const HEADER: usize = 8 + 4 + 32;
const MAX_TEXT: usize = 64 * 1024 * 1024;
const MIN_TEXT: usize = 1024;

#[derive(Debug)]
pub enum StoredText {
    Plain(String),
    Zlib(Vec<u8>),
}
impl ToSql for StoredText {
    fn to_parameter(&self) -> Parameter {
        match self {
            Self::Plain(s) => Parameter::Text(s.clone()),
            Self::Zlib(v) => Parameter::Blob(v.clone()),
        }
    }
}
fn invalid(reason: &str) -> DecapodError {
    DecapodError::ValidationError(format!("LCM_STORAGE_CODEC_CORRUPT: {reason}"))
}
/// Inspect framing only. This does not validate the compressed body or digest.
pub(crate) fn header_decoded_len(bytes: &[u8]) -> Result<usize, DecapodError> {
    if bytes.len() < HEADER || &bytes[..8] != MAGIC {
        return Err(invalid("truncated or unknown header"));
    }
    let expected = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if expected > MAX_TEXT {
        return Err(invalid("decoded size exceeds limit"));
    }
    Ok(expected)
}
pub fn encode(text: &str) -> Result<StoredText, DecapodError> {
    let input = text.as_bytes();
    if input.len() < MIN_TEXT || input.len() > MAX_TEXT {
        return Ok(StoredText::Plain(text.to_owned()));
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(6));
    encoder.write_all(input).map_err(DecapodError::IoError)?;
    let compressed = encoder.finish().map_err(DecapodError::IoError)?;
    let n = HEADER + compressed.len();
    if n.saturating_add(128) >= input.len() || n > input.len().saturating_sub(input.len() / 8) {
        return Ok(StoredText::Plain(text.to_owned()));
    }
    let mut blob = Vec::with_capacity(n);
    blob.extend_from_slice(MAGIC);
    blob.extend_from_slice(&(input.len() as u32).to_le_bytes());
    blob.extend_from_slice(&Sha256::digest(input));
    blob.extend_from_slice(&compressed);
    Ok(StoredText::Zlib(blob))
}
pub fn decode(p: &Parameter) -> Result<String, DecapodError> {
    match p {
        Parameter::Text(s) => Ok(s.clone()),
        Parameter::Blob(bytes) => {
            let expected = header_decoded_len(bytes)?;
            let mut raw = Vec::with_capacity(expected.min(128 * 1024));
            ZlibDecoder::new(&bytes[HEADER..])
                .take(expected as u64 + 1)
                .read_to_end(&mut raw)
                .map_err(|_| invalid("invalid zlib stream"))?;
            if raw.len() != expected || Sha256::digest(&raw).as_slice() != &bytes[12..HEADER] {
                return Err(invalid("decoded size or SHA-256 mismatch"));
            }
            String::from_utf8(raw).map_err(|_| invalid("invalid UTF-8"))
        }
        _ => Err(invalid("unexpected storage type")),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn codec_retains_original_bytes_and_rejects_corruption() {
        let original = "proof  context\r\n".repeat(5000);
        let StoredText::Zlib(mut blob) = encode(&original).unwrap() else {
            panic!("expected compressed test record");
        };
        assert_eq!(decode(&Parameter::Blob(blob.clone())).unwrap(), original);
        blob[14] ^= 1;
        assert!(decode(&Parameter::Blob(blob)).is_err());
        assert_eq!(decode(&Parameter::Text("legacy".into())).unwrap(), "legacy");
        assert!(matches!(encode("tiny").unwrap(), StoredText::Plain(_)));
    }
}
