//! Minimal ULID generator — no external dependencies.
//!
//! Replaces the `ulid` crate (and its `rand` chain) with a small inline
//! implementation: 48-bit ms timestamp | 80-bit OS random, Crockford Base32.

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Generate a new ULID string (26 uppercase Crockford Base32 characters).
pub fn new_ulid() -> String {
    let ts_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let mut rand_bytes = [0u8; 10];
    os_rand(&mut rand_bytes);

    // Pack: 6 bytes timestamp (high 48 bits) | 10 bytes random (low 80 bits)
    let mut raw = [0u8; 16];
    let ts = ts_ms.to_be_bytes();
    raw[0..6].copy_from_slice(&ts[2..8]);
    raw[6..16].copy_from_slice(&rand_bytes);

    encode(u128::from_be_bytes(raw))
}

/// Returns true if `s` is a syntactically valid 26-character ULID.
pub fn is_valid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 26 && b.iter().all(|c| CROCKFORD.contains(c))
}

fn os_rand(buf: &mut [u8]) {
    use std::io::Read;
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        f.read_exact(buf).ok();
    }
}

fn encode(mut value: u128) -> String {
    let mut chars = [0u8; 26];
    for ch in chars.iter_mut().rev() {
        *ch = CROCKFORD[(value & 0x1F) as usize];
        value >>= 5;
    }
    // Safety: all bytes are ASCII from CROCKFORD table
    unsafe { String::from_utf8_unchecked(chars.to_vec()) }
}
