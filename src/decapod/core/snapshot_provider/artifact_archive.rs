//! Byte-authenticated archives only. ZIP members are decoded in memory with
//! finite bounds; no filenames are executed or written to the filesystem.
use super::*;
use std::collections::BTreeMap;
use std::io::Read;
const MAX_ARCHIVE: usize = 256 * 1024 * 1024;
const MAX_MEMBER: usize = 128 * 1024 * 1024;
const MAX_FILES: usize = 1024;
#[derive(Debug)]
pub struct ArtifactFiles {
    origin: String,
    archive_digest: String,
    files: BTreeMap<String, Vec<u8>>,
}
impl ArtifactFiles {
    #[cfg(test)]
    pub(crate) fn fixture(origin: &str, files: BTreeMap<String, Vec<u8>>) -> Self {
        Self {
            origin: origin.into(),
            archive_digest: hash(b"test fixture"),
            files,
        }
    }
    pub fn origin(&self) -> &str {
        &self.origin
    }
    pub fn archive_digest(&self) -> &str {
        &self.archive_digest
    }
    pub fn file(&self, name: &str) -> Result<&[u8]> {
        self.files
            .get(name)
            .map(Vec::as_slice)
            .ok_or_else(|| invalid("authenticated archive member missing"))
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }
    /// Mint only after a provider-specific caller authenticated the exact archive
    /// bytes, source/run/attempt/name/producer and expected SHA256.
    pub(crate) fn verified(
        origin: String,
        expected_digest: &str,
        bytes: &[u8],
        budget: &Budget<'_>,
    ) -> Result<Self> {
        budget.remaining()?;
        ensure(
            bytes.len() <= MAX_ARCHIVE && hash(bytes) == expected_digest,
            "archive bytes differ from provider digest",
        )?;
        Ok(Self {
            origin,
            archive_digest: expected_digest.into(),
            files: unpack(bytes, budget)?,
        })
    }
}
fn u16le(b: &[u8], n: usize) -> Result<u16> {
    let s = b.get(n..n + 2).ok_or_else(|| invalid("truncated ZIP"))?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}
fn u32le(b: &[u8], n: usize) -> Result<u32> {
    let s = b.get(n..n + 4).ok_or_else(|| invalid("truncated ZIP"))?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn signature(b: &[u8], n: usize, s: u32) -> Result<()> {
    ensure(u32le(b, n)? == s, "unsupported ZIP structure")
}
fn member_path(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 1024
        && !name.starts_with('/')
        && !name.contains(['\\', ':'])
        && !name.chars().any(char::is_control)
        && name
            .trim_end_matches('/')
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}
fn unpack(bytes: &[u8], budget: &Budget<'_>) -> Result<BTreeMap<String, Vec<u8>>> {
    budget.remaining()?;
    ensure(
        bytes.len() >= 22 && bytes.len() <= MAX_ARCHIVE,
        "invalid ZIP bound",
    )?;
    let start = bytes.len().saturating_sub(65557);
    let end = (start..=bytes.len() - 22)
        .rev()
        .find(|&n| {
            bytes.get(n..n + 4) == Some(b"PK\x05\x06")
                && u16le(bytes, n + 20)
                    .is_ok_and(|comment| n + 22 + comment as usize == bytes.len())
        })
        .ok_or_else(|| invalid("ZIP end record missing"))?;
    ensure(
        u16le(bytes, end + 4)? == 0 && u16le(bytes, end + 6)? == 0,
        "multipart ZIP unsupported",
    )?;
    let count = u16le(bytes, end + 10)? as usize;
    ensure(
        count > 0 && count <= MAX_FILES && u16le(bytes, end + 8)? as usize == count,
        "ZIP file count unsupported",
    )?;
    let length = u32le(bytes, end + 12)? as usize;
    let offset = u32le(bytes, end + 16)? as usize;
    ensure(
        offset.checked_add(length) == Some(end),
        "ZIP64 or trailing central data unsupported",
    )?;
    let mut cursor = offset;
    let mut total = 0usize;
    let mut files = BTreeMap::new();
    let mut ranges = vec![];
    for _ in 0..count {
        budget.remaining()?;
        signature(bytes, cursor, 0x02014b50)?;
        let flags = u16le(bytes, cursor + 8)?;
        let method = u16le(bytes, cursor + 10)?;
        let crc = u32le(bytes, cursor + 16)?;
        let compressed = u32le(bytes, cursor + 20)? as usize;
        let size = u32le(bytes, cursor + 24)? as usize;
        let name_len = u16le(bytes, cursor + 28)? as usize;
        let extra_len = u16le(bytes, cursor + 30)? as usize;
        let comment_len = u16le(bytes, cursor + 32)? as usize;
        let mode = (u32le(bytes, cursor + 38)? >> 16) & 0xf000;
        let local = u32le(bytes, cursor + 42)? as usize;
        ensure(
            flags & !0x0808 == 0
                && matches!(method, 0 | 8)
                && u16le(bytes, cursor + 34)? == 0
                && matches!(mode, 0 | 0x8000 | 0x4000),
            "encrypted/special/unsupported ZIP member",
        )?;
        ensure(
            size <= MAX_MEMBER && compressed <= MAX_ARCHIVE,
            "ZIP member exceeds bound",
        )?;
        total = total
            .checked_add(size)
            .ok_or_else(|| invalid("ZIP size overflow"))?;
        ensure(total <= MAX_ARCHIVE, "ZIP expanded bytes exceed bound")?;
        let name_end = cursor
            .checked_add(46 + name_len)
            .ok_or_else(|| invalid("ZIP overflow"))?;
        let name = std::str::from_utf8(
            bytes
                .get(cursor + 46..name_end)
                .ok_or_else(|| invalid("ZIP name truncated"))?,
        )
        .map_err(|_| invalid("non-UTF8 ZIP member"))?;
        ensure(member_path(name), "unsafe ZIP member path")?;
        cursor = name_end
            .checked_add(extra_len + comment_len)
            .ok_or_else(|| invalid("ZIP central overflow"))?;
        ensure(cursor <= end, "ZIP central overrun")?;
        ensure(local < offset, "ZIP local header outside data region")?;
        signature(bytes, local, 0x04034b50)?;
        ensure(
            u16le(bytes, local + 6)? == flags && u16le(bytes, local + 8)? == method,
            "ZIP local method/flags mismatch",
        )?;
        let local_name_len = u16le(bytes, local + 26)? as usize;
        let local_extra = u16le(bytes, local + 28)? as usize;
        let data_start = local
            .checked_add(30 + local_name_len + local_extra)
            .ok_or_else(|| invalid("ZIP local overflow"))?;
        let data_end = data_start
            .checked_add(compressed)
            .ok_or_else(|| invalid("ZIP data overflow"))?;
        ensure(
            data_end <= offset
                && bytes.get(local + 30..local + 30 + local_name_len) == Some(name.as_bytes()),
            "ZIP local name/range mismatch",
        )?;
        if flags & 8 == 0 {
            ensure(
                u32le(bytes, local + 14)? == crc
                    && u32le(bytes, local + 18)? as usize == compressed
                    && u32le(bytes, local + 22)? as usize == size,
                "ZIP local size/CRC mismatch",
            )?;
        }
        ensure(
            !ranges.iter().any(|&(a, b)| local < b && a < data_end),
            "overlapping ZIP members",
        )?;
        ranges.push((local, data_end));
        if name.ends_with('/') {
            ensure(size == 0 && mode != 0x8000, "malformed ZIP directory")?;
            continue;
        }
        ensure(mode != 0x4000, "ZIP directory mode on file")?;
        let mut output = Vec::with_capacity(size.min(1024 * 1024));
        if method == 0 {
            ensure(compressed == size, "stored ZIP size mismatch")?;
            output.extend_from_slice(&bytes[data_start..data_end]);
        } else {
            let mut decoder = flate2::read::DeflateDecoder::new(&bytes[data_start..data_end]);
            let mut chunk = [0u8; 65536];
            loop {
                budget.remaining()?;
                let n = decoder.read(&mut chunk)?;
                if n == 0 {
                    break;
                }
                ensure(
                    output.len() + n <= size,
                    "ZIP expansion exceeded declared bound",
                )?;
                output.extend_from_slice(&chunk[..n]);
            }
            ensure(
                decoder.total_in() == compressed as u64,
                "ZIP compressed trailing data",
            )?;
        }
        let mut actual_crc = flate2::Crc::new();
        actual_crc.update(&output);
        ensure(
            output.len() == size && actual_crc.sum() == crc,
            "ZIP member content mismatch",
        )?;
        ensure(
            files.insert(name.into(), output).is_none(),
            "duplicate ZIP member",
        )?;
    }
    ensure(
        cursor == end && !files.is_empty(),
        "ZIP central inventory mismatch",
    )?;
    Ok(files)
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn zip(name: &str, data: &[u8], mode: u32) -> Vec<u8> {
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let mut out = vec![];
        let w16 = |v: u16, o: &mut Vec<u8>| o.extend(v.to_le_bytes());
        let w32 = |v: u32, o: &mut Vec<u8>| o.extend(v.to_le_bytes());
        w32(0x04034b50, &mut out);
        for v in [20, 0, 0, 0, 0] {
            w16(v, &mut out);
        }
        for v in [crc.sum(), data.len() as u32, data.len() as u32] {
            w32(v, &mut out);
        }
        w16(name.len() as u16, &mut out);
        w16(0, &mut out);
        out.extend(name.as_bytes());
        out.extend(data);
        let off = out.len();
        w32(0x02014b50, &mut out);
        for v in [20, 20, 0, 0, 0, 0] {
            w16(v, &mut out);
        }
        for v in [crc.sum(), data.len() as u32, data.len() as u32] {
            w32(v, &mut out);
        }
        for v in [name.len() as u16, 0, 0, 0, 0] {
            w16(v, &mut out);
        }
        w32(mode << 16, &mut out);
        w32(0, &mut out);
        out.extend(name.as_bytes());
        let size = out.len() - off;
        w32(0x06054b50, &mut out);
        for v in [0, 0, 1, 1] {
            w16(v, &mut out);
        }
        w32(size as u32, &mut out);
        w32(off as u32, &mut out);
        w16(0, &mut out);
        out
    }
    #[test]
    fn safe_archive_has_no_disk_extraction() {
        let c = Cancellation::default();
        let b = Budget::new(Duration::from_secs(1), 1024, &c).unwrap();
        let z = zip("execution/execution.json", b"{}", 0x8000);
        let a = ArtifactFiles::verified("provider:job".into(), &hash(&z), &z, &b).unwrap();
        assert_eq!(a.file("execution/execution.json").unwrap(), b"{}");
    }
    #[test]
    fn path_escape_symlink_and_tampering_reject() {
        let c = Cancellation::default();
        let b = Budget::new(Duration::from_secs(1), 1024, &c).unwrap();
        for (name, mode) in [
            ("../escape", 0x8000),
            ("/absolute", 0x8000),
            ("link", 0xa000),
            ("a\\b", 0x8000),
        ] {
            let z = zip(name, b"x", mode);
            assert!(ArtifactFiles::verified("p".into(), &hash(&z), &z, &b).is_err());
        }
        let z = zip("file", b"x", 0x8000);
        assert!(ArtifactFiles::verified("p".into(), &hash(b"other"), &z, &b).is_err());
    }
}
