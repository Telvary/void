//! Cross-platform IPC endpoint paths for the WhatsApp RPC server.

use std::path::{Path, PathBuf};

#[cfg(unix)]
fn store_hash(store_path: &Path) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    store_path.hash(&mut hasher);
    hasher.finish()
}

/// Unix domain socket path. Uses `/tmp` to stay within `SUN_LEN` on macOS.
#[cfg(unix)]
pub fn endpoint_path(store_path: &Path) -> PathBuf {
    PathBuf::from(format!("/tmp/void-wa-{:x}.sock", store_hash(store_path)))
}

/// Named pipe identifier derived from the store path (Windows).
#[cfg(windows)]
pub fn endpoint_path(store_path: &Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    store_path.hash(&mut hasher);
    format!(r"\\.\pipe\void-wa-rpc-{:x}", hasher.finish())
}

/// Remove a stale Unix socket before binding.
#[cfg(unix)]
pub fn remove_stale_endpoint(path: &Path) {
    if path.exists() {
        std::fs::remove_file(path).ok();
    }
}

#[cfg(windows)]
pub fn remove_stale_endpoint(_path: &str) {}
