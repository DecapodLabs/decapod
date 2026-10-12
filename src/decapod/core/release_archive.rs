//! Bounded tar inspection. This never extracts links, special files or paths.
//! Unknown archive extensions fail closed. The build adapter decompresses xz
//! into its owned scratch; gzip inputs use the existing flate2 dependency.
use std::collections::BTreeMap;
use std::io::{self, Read};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub mode: u32,
    pub bytes: Vec<u8>,
}
fn invalid(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s)
}
fn text(bytes: &[u8]) -> io::Result<String> {
    let bytes = bytes.split(|b| *b == 0).next().unwrap_or_default();
    std::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| invalid("non-UTF-8 archive path"))
}
fn octal(bytes: &[u8]) -> io::Result<u64> {
    if bytes.first().is_some_and(|b| b & 0x80 != 0) {
        return Err(invalid("base-256 tar number unsupported"));
    }
    let text = text(bytes)?;
    let n = text.trim();
    u64::from_str_radix(if n.is_empty() { "0" } else { n }, 8)
        .map_err(|_| invalid("invalid tar number"))
}
fn safe(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && path
            .trim_end_matches('/')
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}
fn pax(bytes: &[u8]) -> io::Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let space = rest
            .iter()
            .position(|b| *b == b' ')
            .ok_or_else(|| invalid("invalid PAX record"))?;
        let len = std::str::from_utf8(&rest[..space])
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .ok_or_else(|| invalid("invalid PAX length"))?;
        if len <= space + 2 || len > rest.len() || rest[len - 1] != b'\n' {
            return Err(invalid("invalid PAX record size"));
        }
        let entry =
            std::str::from_utf8(&rest[space + 1..len - 1]).map_err(|_| invalid("non-UTF-8 PAX"))?;
        let (k, v) = entry
            .split_once('=')
            .ok_or_else(|| invalid("invalid PAX entry"))?;
        if !matches!(k, "path" | "mtime" | "atime" | "ctime" | "comment")
            || result.insert(k.into(), v.into()).is_some()
        {
            return Err(invalid("unsupported or duplicate PAX attribute"));
        }
        rest = &rest[len..];
    }
    Ok(result)
}
/// Return regular files only. Directories are checked; links/special entries fail.
pub fn inspect(
    mut input: impl Read,
    max_bytes: u64,
    max_members: usize,
) -> io::Result<BTreeMap<String, Member>> {
    if max_bytes == 0 || max_members == 0 {
        return Err(invalid("empty archive budget"));
    }
    let mut files = BTreeMap::new();
    let mut names = std::collections::BTreeSet::new();
    let mut consumed = 0u64;
    let mut pending_path = None;
    let mut zeros = 0;
    loop {
        let mut header = [0u8; 512];
        input.read_exact(&mut header)?;
        consumed = consumed
            .checked_add(512)
            .ok_or_else(|| invalid("archive budget overflow"))?;
        if consumed > max_bytes {
            return Err(invalid("archive exceeds byte budget"));
        }
        if header == [0; 512] {
            zeros += 1;
            if zeros == 2 {
                let mut remaining = [0u8; 4096];
                loop {
                    let n = input.read(&mut remaining)?;
                    if n == 0 {
                        break;
                    }
                    consumed += n as u64;
                    if consumed > max_bytes || remaining[..n].iter().any(|b| *b != 0) {
                        return Err(invalid("trailing tar material"));
                    }
                }
                if pending_path.is_some() {
                    return Err(invalid("orphaned extended tar header"));
                }
                return Ok(files);
            }
            continue;
        }
        if zeros != 0 {
            return Err(invalid("truncated archive terminator"));
        }
        let expected = octal(&header[148..156])?;
        let actual: u64 = header
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    *b as u64
                }
            })
            .sum();
        if expected != actual {
            return Err(invalid("tar header checksum mismatch"));
        }
        let size = octal(&header[124..136])?;
        let padded = size
            .checked_add(511)
            .ok_or_else(|| invalid("tar size overflow"))?
            / 512
            * 512;
        consumed = consumed
            .checked_add(padded)
            .ok_or_else(|| invalid("archive budget overflow"))?;
        if consumed > max_bytes || size > usize::MAX as u64 {
            return Err(invalid("archive exceeds byte budget"));
        }
        let mut bytes = vec![0; size as usize];
        input.read_exact(&mut bytes)?;
        let mut pad = vec![0; (padded - size) as usize];
        input.read_exact(&mut pad)?;
        let kind = header[156];
        if matches!(kind, b'x' | b'g') {
            let attributes = pax(&bytes)?;
            if kind == b'g' && attributes.contains_key("path") {
                return Err(invalid("global PAX path unsupported"));
            }
            if let Some(path) = attributes.get("path")
                && pending_path.replace(path.clone()).is_some()
            {
                return Err(invalid("duplicate extended path"));
            }
            continue;
        }
        if kind == b'L' {
            if pending_path.replace(text(&bytes)?).is_some() {
                return Err(invalid("duplicate GNU long name"));
            }
            continue;
        }
        let name = if let Some(path) = pending_path.take() {
            path
        } else {
            let basename = text(&header[..100])?;
            let prefix = text(&header[345..500])?;
            if prefix.is_empty() {
                basename
            } else {
                format!("{prefix}/{basename}")
            }
        };
        if !safe(&name)
            || !names.insert(name.trim_end_matches('/').to_owned())
            || names.len() > max_members
        {
            return Err(invalid("unsafe, duplicate or excess archive member"));
        }
        match kind {
            0 | b'0' => {
                let mode = octal(&header[100..108])? as u32;
                if mode & !0o777 != 0 {
                    return Err(invalid("unsafe archive file mode"));
                }
                files.insert(name, Member { mode, bytes });
            }
            b'5' if size == 0 => {}
            _ => return Err(invalid("archive links or special members unsupported")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tar(entries: &[(&str, u8, &[u8])]) -> Vec<u8> {
        let mut tar = Vec::new();
        for (name, kind, bytes) in entries {
            let mut h = [0u8; 512];
            h[..name.len()].copy_from_slice(name.as_bytes());
            h[100..108].copy_from_slice(b"0000644\0");
            h[124..136].copy_from_slice(format!("{:011o}\0", bytes.len()).as_bytes());
            h[148..156].fill(b' ');
            h[156] = *kind;
            h[257..263].copy_from_slice(b"ustar\0");
            let sum: u32 = h.iter().map(|b| *b as u32).sum();
            h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
            tar.extend(h);
            tar.extend(*bytes);
            tar.resize(tar.len().next_multiple_of(512), 0);
        }
        tar.resize(tar.len() + 1024, 0);
        tar
    }
    #[test]
    fn regular_archive_is_read_without_extracting() {
        let t = tar(&[("root/file", b'0', b"hello")]);
        let m = inspect(t.as_slice(), 4096, 10).unwrap();
        assert_eq!(m["root/file"].bytes, b"hello");
    }
    #[test]
    fn unsafe_paths_links_duplicates_and_special_files_fail() {
        for name in ["../escape", "/absolute", "root/../escape", "root\\escape"] {
            assert!(inspect(tar(&[(name, b'0', b"bad")]).as_slice(), 4096, 10).is_err());
        }
        for kind in [b'1', b'2', b'3', b'4', b'6', b'K'] {
            assert!(inspect(tar(&[("root/item", kind, b"")]).as_slice(), 4096, 10).is_err());
        }
        assert!(
            inspect(
                tar(&[("root/file", b'0', b"a"), ("root/file", b'0', b"b")]).as_slice(),
                4096,
                10
            )
            .is_err()
        );
    }
    #[test]
    fn bounds_truncation_checksum_and_trailing_bytes_fail() {
        let t = tar(&[("root/file", b'0', b"hello")]);
        assert!(inspect(t.as_slice(), 512, 10).is_err());
        assert!(inspect(&t[..t.len() - 513], 4096, 10).is_err());
        let mut bad = t.clone();
        bad[0] = b'X';
        assert!(inspect(bad.as_slice(), 4096, 10).is_err());
        let mut bad = t;
        bad.push(1);
        assert!(inspect(bad.as_slice(), 4096, 10).is_err());
    }
    #[test]
    fn pax_length_and_path_are_strict() {
        assert!(pax(b"999 path=../escape\n").is_err());
        assert!(pax(b"17 linkpath=other\n").is_err());
    }
}
