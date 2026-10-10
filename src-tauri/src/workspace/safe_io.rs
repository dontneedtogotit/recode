//! Descriptor-relative file IO for untrusted workspace paths.
//!
//! Every path component is resolved with `openat(.., O_NOFOLLOW |
//! O_DIRECTORY)` from an already-open directory descriptor, so a symlink
//! swapped in concurrently cannot redirect IO outside the workspace: each
//! step either lands on a real directory whose descriptor we hold, or fails
//! with `ELOOP`/`ENOTDIR`. This is the portable equivalent of
//! `openat2(RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS)` for kernels without
//! `openat2`.

use std::ffi::CString;
use std::io;
use std::io::Read;
use std::io::Write;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use rustix::fd::{AsFd, OwnedFd};
use rustix::fs::{fchmod, fstat, fsync, mkdirat, openat, renameat, unlinkat, AtFlags, Mode, OFlags};

// POSIX file-type bits; stable constants, no libc dependency needed.
const S_IFMT: u32 = 0o170_000;
const S_IFREG: u32 = 0o100_000;
const S_IFDIR: u32 = 0o040_000;

fn open_flags() -> OFlags {
    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}

fn dir_flags() -> OFlags {
    OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}

fn user_rw() -> Mode {
    Mode::RUSR | Mode::WUSR
}

fn user_rwx() -> Mode {
    Mode::RUSR | Mode::WUSR | Mode::XUSR
}

fn mode_from_bits(bits: u32) -> Mode {
    let mut mode = Mode::empty();
    for (bit, flag) in [
        (0o400, Mode::RUSR),
        (0o200, Mode::WUSR),
        (0o100, Mode::XUSR),
        (0o040, Mode::RGRP),
        (0o020, Mode::WGRP),
        (0o010, Mode::XGRP),
        (0o004, Mode::ROTH),
        (0o002, Mode::WOTH),
        (0o001, Mode::XOTH),
    ] {
        if bits & bit != 0 {
            mode |= flag;
        }
    }
    mode
}

fn cstr(component: &str) -> io::Result<CString> {
    CString::new(component.as_bytes()).map_err(|_| io::Error::other("path component contains NUL"))
}

/// A workspace root held open as a directory descriptor. All operations
/// beneath it resolve relative to this descriptor, never by re-walking the
/// filesystem namespace from a path string.
pub struct RootDir {
    fd: OwnedFd,
}

impl RootDir {
    /// Open `root` as a directory descriptor and verify the descriptor still
    /// names the directory the path named at open time (device + inode match
    /// against non-following metadata). A concurrent swap of the root to a
    /// symlink, a file, or a missing directory is therefore rejected.
    /// Swapping in a *different real directory* with the same name requires
    /// write access to the parent projects directory — the registrar's own
    /// authority — and is enforced at registration time.
    pub fn open(root: &Path) -> io::Result<Self> {
        let file = std::fs::File::open(root)?;
        let fd: OwnedFd = file.into();
        let stat = fstat(&fd).map_err(errno)?;
        let named = std::fs::symlink_metadata(root)?;
        if named.file_type().is_symlink()
            || stat.st_mode & S_IFMT != S_IFDIR
            || stat.st_dev != named.dev()
            || stat.st_ino != named.ino()
        {
            return Err(io::Error::other("workspace root changed while opening"));
        }
        Ok(Self { fd })
    }

    /// Resolve `relative` to the descriptor of its parent directory plus the
    /// final component name. Missing intermediate directories are created and
    /// then re-opened without following links when `create` is set; a symlink
    /// swapped in for a created directory fails the re-open with `ELOOP`.
    fn open_parent(&self, relative: &str, create: bool) -> io::Result<(OwnedFd, CString)> {
        let components: Vec<&str> = relative.split('/').collect();
        debug_assert!(!components.is_empty(), "validated paths are non-empty");
        let mut current = self.fd.try_clone()?;
        for (index, component) in components.iter().enumerate() {
            if index + 1 == components.len() {
                return Ok((current, cstr(component)?));
            }
            let name = cstr(component)?;
            match open_dir_at(&current, &name) {
                Ok(dir) => current = dir,
                Err(error) if create && error.kind() == io::ErrorKind::NotFound => {
                    mkdirat(&current, name.clone(), user_rwx()).map_err(errno)?;
                    current = open_dir_at(&current, &name)?;
                }
                Err(error) => return Err(error),
            }
        }
        unreachable!("paths always have at least one component")
    }

    /// Read the final component of `relative` into a bounded buffer, refusing
    /// symlinks and non-regular files, and re-checking the size after the
    /// open so a grown file cannot slip past the limit.
    pub fn read_file(&self, relative: &str, max_bytes: usize) -> io::Result<Vec<u8>> {
        let (parent, name) = self.open_parent(relative, false)?;
        let fd = openat(parent.as_fd(), name, open_flags(), Mode::empty()).map_err(errno)?;
        let stat = fstat(&fd).map_err(errno)?;
        if stat.st_mode & S_IFMT != S_IFREG {
            return Err(io::Error::other("workspace path is not a regular file"));
        }
        if stat.st_size as u64 > max_bytes as u64 {
            return Err(io::Error::other("file exceeds the size limit"));
        }
        let file = std::fs::File::from(fd);
        let mut bytes = Vec::new();
        file.take(max_bytes as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > max_bytes {
            return Err(io::Error::other("file exceeds the size limit"));
        }
        Ok(bytes)
    }

    /// Atomically replace `relative` with `content`: create a temporary file
    /// in the same directory (O_EXCL, O_NOFOLLOW), preserve the permissions
    /// of an existing regular file, fsync the content, rename over the
    /// target, and fsync the containing directory so the rename is durable.
    /// Symlinks are never followed at any step; `renameat` replaces a link
    /// rather than writing through it.
    pub fn write_file(&self, relative: &str, content: &[u8], max_bytes: usize) -> io::Result<()> {
        if content.len() > max_bytes {
            return Err(io::Error::other("content exceeds the size limit"));
        }
        let (parent, name) = self.open_parent(relative, true)?;
        let temporary =
            cstr(&format!(".recode-write-{}.tmp", uuid::Uuid::new_v4())).expect("temp name is valid");
        let result = write_through(&parent, &temporary, &name, content);
        if result.is_err() {
            let _ = unlinkat(parent.as_fd(), temporary, AtFlags::empty());
        }
        result
    }
}

/// Convert a rustix `Errno` into an `io::Error`, preserving the code.
fn errno(error: rustix::io::Errno) -> io::Error {
    io::Error::from_raw_os_error(error.raw_os_error())
}

fn open_dir_at(parent: &OwnedFd, name: &CString) -> io::Result<OwnedFd> {
    openat(parent.as_fd(), name.clone(), dir_flags(), Mode::empty()).map_err(errno)
}

fn write_through(
    parent: &OwnedFd,
    temporary: &CString,
    name: &CString,
    content: &[u8],
) -> io::Result<()> {
    let tmp_fd = openat(
        parent.as_fd(),
        temporary.clone(),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        user_rw(),
    )
    .map_err(errno)?;
    // Preserve the permissions of an existing regular file and refuse to
    // replace anything else; the O_NOFOLLOW open also fails on symlinks.
    match openat(parent.as_fd(), name.clone(), open_flags(), Mode::empty())
        .map_err(errno)
    {
        Ok(existing) => {
            let stat = fstat(&existing).map_err(errno)?;
            if stat.st_mode & S_IFMT != S_IFREG {
                return Err(io::Error::other("refusing to replace a non-regular file"));
            }
            fchmod(&tmp_fd, mode_from_bits(stat.st_mode & 0o777)).map_err(errno)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut file = std::fs::File::from(tmp_fd);
    file.write_all(content)?;
    file.sync_all()?;
    drop(file);
    renameat(parent.as_fd(), temporary.clone(), parent.as_fd(), name.clone()).map_err(errno)?;
    // Make the rename durable: the directory entry must reach stable storage
    // or a crash can resurrect the previous file contents.
    fsync(parent.as_fd()).map_err(errno)?;
    Ok(())
}

/// Flush a directory's entries to stable storage.
pub fn sync_directory(path: &Path) -> io::Result<()> {
    let file = std::fs::File::open(path)?;
    let fd: OwnedFd = file.into();
    fsync(&fd).map_err(errno)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_rejects_missing_and_symlinked_roots_but_accepts_real_directories() {
        let base = std::env::temp_dir().join(format!("recode-rootfd-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        assert!(RootDir::open(&base.join("missing")).is_err());
        let real = base.join("real");
        std::fs::create_dir_all(&real).unwrap();
        assert!(RootDir::open(&real).is_ok());
        let outside = base.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, base.join("linked")).unwrap();
        assert!(RootDir::open(&base.join("linked")).is_err());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn read_and_write_files_beneath_the_descriptor() {
        let base = std::env::temp_dir().join(format!("recode-rootio-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        let root = RootDir::open(&base).unwrap();
        root.write_file("nested/dir/file.txt", b"hello", 1024).unwrap();
        assert_eq!(root.read_file("nested/dir/file.txt", 1024).unwrap(), b"hello");
        assert!(root.read_file("missing.txt", 1024).is_err());
        assert!(root.read_file("nested/dir/file.txt", 2).is_err());
        assert!(root.write_file("nested/dir/file.txt", b"x".repeat(2048).as_slice(), 1024).is_err());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn symlink_swapped_components_cannot_redirect_io() {
        let base = std::env::temp_dir().join(format!("recode-rootswap-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(base.join("outside")).unwrap();
        let root = RootDir::open(&base).unwrap();
        // A symlink swapped in for an intermediate directory must fail the
        // O_NOFOLLOW open, not be traversed.
        std::os::unix::fs::symlink(base.join("outside"), base.join("evil")).unwrap();
        assert!(root.read_file("evil/secret.txt", 1024).is_err());
        assert!(root.write_file("evil/secret.txt", b"pwned", 1024).is_err());
        assert!(!base.join("outside/secret.txt").exists());
        // A symlink swapped in for the final component must fail, not be
        // followed or written through.
        std::os::unix::fs::symlink(base.join("outside/target.txt"), base.join("link.txt")).unwrap();
        assert!(root.write_file("link.txt", b"pwned", 1024).is_err());
        assert!(root.read_file("link.txt", 1024).is_err());
        assert!(!base.join("outside/target.txt").exists());
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn writes_never_replace_non_regular_files_or_symlinked_directories() {
        let base = std::env::temp_dir().join(format!("recode-rootnreg-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(base.join("outside")).unwrap();
        let root = RootDir::open(&base).unwrap();
        std::fs::create_dir(base.join("dir.txt")).unwrap();
        // A directory at the target path is refused, not traversed into.
        assert!(root.write_file("dir.txt", b"no", 1024).is_err());
        // A FIFO or device file would also be refused by the regular-file
        // check; a symlink at the final component is refused by O_NOFOLLOW.
        std::os::unix::fs::symlink(base.join("outside"), base.join("dirlink")).unwrap();
        assert!(root.write_file("dirlink/file.txt", b"no", 1024).is_err());
        assert!(root.read_file("dirlink/file.txt", 1024).is_err());
        let _ = std::fs::remove_dir_all(base);
    }
}
