//! macOS cannot enumerate directory descriptors through /dev/fd. Keep the
//! directory stream descriptor-relative instead of reopening an observed path.
use crate::core::project_specs::{RepoSignalEntries, RepoSignalEntry};
use std::{
    ffi::CStr,
    fs::File,
    io,
    os::fd::{AsRawFd, FromRawFd},
    os::unix::ffi::OsStrExt,
    ptr::NonNull,
};

struct DirectoryStream {
    stream: NonNull<libc::DIR>,
    finished: bool,
}

pub(super) fn entries(directory: &File) -> io::Result<RepoSignalEntries> {
    // A new open description gives each traversal an independent cursor.
    // dup/try_clone would share the root descriptor's directory offset.
    let fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            c".".as_ptr(),
            libc::O_RDONLY
                | libc::O_DIRECTORY
                | libc::O_NOFOLLOW
                | libc::O_NONBLOCK
                | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // Own the descriptor until fdopendir successfully takes ownership.
    let file = unsafe { File::from_raw_fd(fd) };
    let stream = NonNull::new(unsafe { libc::fdopendir(file.as_raw_fd()) })
        .ok_or_else(io::Error::last_os_error)?;
    std::mem::forget(file);
    Ok(Box::new(DirectoryStream {
        stream,
        finished: false,
    }))
}

impl Iterator for DirectoryStream {
    type Item = io::Result<RepoSignalEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        loop {
            // POSIX distinguishes EOF from failure through errno. The stream
            // is exclusively owned; copy the entry before the next readdir.
            unsafe {
                *libc::__error() = 0;
            }
            let entry = unsafe { libc::readdir(self.stream.as_ptr()) };
            if entry.is_null() {
                self.finished = true;
                let error = io::Error::last_os_error();
                return (error.raw_os_error() != Some(0)).then_some(Err(error));
            }
            let name = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
            if matches!(name.to_bytes(), b"." | b"..") {
                continue;
            }
            // Do not trust d_type (which may be DT_UNKNOWN). lstat relative to
            // the same held directory classifies links without following them.
            let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
            let result = unsafe {
                libc::fstatat(
                    libc::dirfd(self.stream.as_ptr()),
                    name.as_ptr(),
                    stat.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if result < 0 {
                self.finished = true;
                return Some(Err(io::Error::last_os_error()));
            }
            let mode = unsafe { stat.assume_init() }.st_mode & libc::S_IFMT;
            return Some(Ok(RepoSignalEntry {
                name: std::ffi::OsStr::from_bytes(name.to_bytes()).to_owned(),
                is_dir: mode == libc::S_IFDIR,
                is_file: mode == libc::S_IFREG,
                is_symlink: mode == libc::S_IFLNK,
            }));
        }
    }
}

impl Drop for DirectoryStream {
    fn drop(&mut self) {
        // closedir closes the descriptor transferred by fdopendir exactly once.
        unsafe {
            libc::closedir(self.stream.as_ptr());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, fs, os::unix::fs::symlink};

    fn names(directory: &File) -> BTreeSet<std::ffi::OsString> {
        entries(directory)
            .unwrap()
            .map(|entry| entry.unwrap().name)
            .collect()
    }

    #[test]
    fn independent_streams_do_not_share_a_directory_cursor() {
        let root = tempfile::tempdir().unwrap();
        for name in ["a", "b", "c"] {
            fs::write(root.path().join(name), name).unwrap();
        }
        let directory = File::open(root.path()).unwrap();
        let expected = names(&directory);
        assert_eq!(expected.len(), 3);
        let mut partial = entries(&directory).unwrap();
        assert!(partial.next().unwrap().is_ok());
        assert_eq!(names(&directory), expected);
        drop(partial);
        assert_eq!(names(&directory), expected);
    }

    #[test]
    fn stream_survives_path_replacement_and_original_handle_drop() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("source");
        let moved = root.path().join("pinned");
        let hostile = root.path().join("hostile");
        fs::create_dir(&path).unwrap();
        fs::create_dir(&hostile).unwrap();
        fs::write(path.join("original.txt"), "original").unwrap();
        fs::write(hostile.join("forged.txt"), "forged").unwrap();
        let directory = File::open(&path).unwrap();
        let stream = entries(&directory).unwrap();
        drop(directory);
        fs::rename(&path, &moved).unwrap();
        symlink(&hostile, &path).unwrap();
        let entries = stream.collect::<io::Result<Vec<_>>>().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "original.txt");
        assert!(entries[0].is_file);
        assert!(!entries[0].is_symlink);
    }

    #[test]
    fn descriptor_relative_metadata_preserves_file_directory_and_link_kinds() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("directory")).unwrap();
        fs::write(root.path().join("file"), "data").unwrap();
        symlink("file", root.path().join("link")).unwrap();
        let directory = File::open(root.path()).unwrap();
        for entry in entries(&directory).unwrap() {
            let entry = entry.unwrap();
            match entry.name.to_str().unwrap() {
                "directory" => assert!(entry.is_dir && !entry.is_file && !entry.is_symlink),
                "file" => assert!(!entry.is_dir && entry.is_file && !entry.is_symlink),
                "link" => assert!(!entry.is_dir && !entry.is_file && entry.is_symlink),
                name => panic!("unexpected entry: {name}"),
            }
        }
    }
}
