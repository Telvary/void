//! Filesystem helpers that keep secret-bearing files owner-only.
//!
//! Credentials at rest (config, OAuth tokens, session keys) must never be
//! world-readable: on a multi-user host anyone with read access to the file
//! could act as the user. These helpers write such files with `0600` and
//! their parent directory with `0700` on Unix.

use std::io::{self, Write};
use std::path::Path;

/// Write `contents` to `path` with owner-only permissions (`0600` on Unix).
///
/// The bytes are written to a temporary sibling created with restricted
/// permissions *before* any secret touches a visible path, then atomically
/// renamed into place. The parent directory is created if needed and, on Unix,
/// restricted to `0700` (best effort).
pub fn write_secure(path: &Path, contents: impl AsRef<[u8]>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
            restrict_dir(parent);
        }
    }

    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    let mut tmp_name = file_name.to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);

    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }

    let mut file = opts.open(&tmp)?;
    file.write_all(contents.as_ref())?;
    file.sync_all()?;
    drop(file);

    std::fs::rename(&tmp, path)
}

/// Restrict an already-existing file to owner-only (`0600` on Unix).
///
/// Use for files created by third-party code (e.g. a SQLite store opened by a
/// dependency) that cannot go through [`write_secure`]. Best effort: errors are
/// returned so the caller can decide whether to surface them.
#[cfg(unix)]
pub fn restrict_file(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
pub fn restrict_file(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn restrict_dir(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
fn restrict_dir(_dir: &Path) {}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn write_secure_creates_owner_only_file() {
        let dir = std::env::temp_dir().join(format!("void-secure-{}", std::process::id()));
        let path = dir.join("secret.json");
        write_secure(&path, b"top-secret").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "file must be owner-only");
        assert_eq!(std::fs::read(&path).unwrap(), b"top-secret");

        let dir_mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(dir_mode & 0o777, 0o700, "dir must be owner-only");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_secure_overwrites_and_tightens_existing_file() {
        let dir = std::env::temp_dir().join(format!("void-secure-ow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("token.json");
        std::fs::write(&path, b"old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        write_secure(&path, b"new").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(std::fs::read(&path).unwrap(), b"new");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
