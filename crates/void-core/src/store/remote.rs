use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::expand_tilde;
use crate::error::ConfigError;

/// Prepended to remote SSH commands so `void` and user tools are discoverable.
pub const REMOTE_PATH_PREFIX: &str = "PATH=\"$HOME/bin:$HOME/.local/bin:$HOME/.cargo/bin:$PATH\"";

#[derive(Debug, Clone)]
pub struct RemoteProxyTargets {
    pub config_path: String,
    pub void_bin: String,
}

#[derive(Debug, Clone)]
pub struct SshTarget {
    pub host: String,
    pub user: Option<String>,
    pub port: u16,
    pub identity_file: Option<PathBuf>,
}

impl SshTarget {
    pub fn destination(&self) -> String {
        match &self.user {
            Some(user) => format!("{user}@{}", self.host),
            None => self.host.clone(),
        }
    }

    fn base_ssh_args(&self) -> Vec<String> {
        let mut args = vec![
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            "StrictHostKeyChecking=accept-new".into(),
            "-p".into(),
            self.port.to_string(),
        ];
        if let Some(identity) = &self.identity_file {
            args.push("-i".into());
            args.push(identity.to_string_lossy().into_owned());
        }
        args
    }

    /// Resolve `~/…` using the remote host's `$HOME` (for SSH-proxied CLI commands).
    pub fn resolve_path_on_host(&self, path: &str) -> Result<String, ConfigError> {
        if let Some(rest) = path.strip_prefix("~/") {
            let home = self.remote_home_dir()?;
            Ok(home.join(rest).to_string_lossy().into_owned())
        } else if path == "~" {
            Ok(self.remote_home_dir()?.to_string_lossy().into_owned())
        } else {
            Ok(path.to_string())
        }
    }

    /// Resolve absolute config path and `void` binary on the remote host (one SSH round-trip).
    pub fn resolve_proxy_targets(
        &self,
        config_path: &str,
    ) -> Result<RemoteProxyTargets, ConfigError> {
        let output = self.run_remote(&format!(
            "{REMOTE_PATH_PREFIX}; \
             home=$(printf %s \"$HOME\"); \
             bin=$(command -v void); \
             printf '%s\n%s\n' \"$home\" \"$bin\""
        ))?;
        if !output.status.success() {
            return Err(ConfigError::Remote(
                "failed to resolve remote $HOME and void binary".into(),
            ));
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| ConfigError::Remote(format!("invalid proxy resolve output: {e}")))?;
        let mut lines = stdout.lines();
        let home = lines
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ConfigError::Remote("remote $HOME is empty".into()))?;
        let void_bin = lines
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ConfigError::Remote(
                    "void not found on remote host (install to ~/bin/void or ~/.local/bin/void)"
                        .into(),
                )
            })?;

        let resolved_config = if let Some(rest) = config_path.strip_prefix("~/") {
            format!("{home}/{rest}")
        } else if config_path == "~" {
            home.to_string()
        } else {
            config_path.to_string()
        };

        Ok(RemoteProxyTargets {
            config_path: resolved_config,
            void_bin: void_bin.to_string(),
        })
    }

    pub fn resolve_void_bin(&self) -> Result<String, ConfigError> {
        let output = self.run_remote(&format!("{REMOTE_PATH_PREFIX}; command -v void"))?;
        if !output.status.success() {
            return Err(ConfigError::Remote(
                "void not found on remote host (install to ~/bin/void or ~/.local/bin/void)".into(),
            ));
        }
        let bin = String::from_utf8(output.stdout)
            .map_err(|e| ConfigError::Remote(format!("invalid remote void path: {e}")))?
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if bin.is_empty() {
            return Err(ConfigError::Remote(
                "void not found on remote host (install to ~/bin/void or ~/.local/bin/void)".into(),
            ));
        }
        Ok(bin)
    }

    fn remote_home_dir(&self) -> Result<PathBuf, ConfigError> {
        let output = self.run_remote("printf %s \"$HOME\"")?;
        if !output.status.success() {
            return Err(ConfigError::Remote(
                "failed to resolve remote $HOME for config path".into(),
            ));
        }
        let home = String::from_utf8(output.stdout)
            .map_err(|e| ConfigError::Remote(format!("invalid remote $HOME: {e}")))?
            .trim()
            .to_string();
        if home.is_empty() {
            return Err(ConfigError::Remote("remote $HOME is empty".into()));
        }
        Ok(PathBuf::from(home))
    }

    pub fn run_remote(&self, remote_command: &str) -> Result<Output, ConfigError> {
        let mut cmd = Command::new("ssh");
        cmd.args(self.base_ssh_args());
        cmd.arg(self.destination());
        cmd.arg(remote_command);
        cmd.output()
            .map_err(|e| ConfigError::Remote(format!("ssh failed: {e}")))
    }

    pub fn scp_to(&self, local_path: &Path, remote_path: &str) -> Result<(), ConfigError> {
        // scp does not expand `~` on the remote destination; resolve to an absolute path.
        let resolved_remote = if remote_path.starts_with('~') {
            self.resolve_path_on_host(remote_path)?
        } else {
            remote_path.to_string()
        };
        let remote_spec = format!(
            "{}:{}",
            self.destination(),
            format_remote_scp_path(&resolved_remote)
        );
        let mut cmd = Command::new("scp");
        cmd.args(self.base_scp_args());
        cmd.arg(local_path);
        cmd.arg(&remote_spec);
        let output = cmd
            .output()
            .map_err(|e| ConfigError::Remote(format!("scp failed: {e}")))?;
        if output.status.success() {
            return Ok(());
        }
        Err(ConfigError::Remote(format!(
            "scp failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )))
    }

    pub fn scp_from(&self, remote_path: &str, local_path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let remote_spec = format!(
            "{}:{}",
            self.destination(),
            format_remote_scp_path(remote_path)
        );
        let mut cmd = Command::new("scp");
        cmd.args(self.base_scp_args());
        cmd.arg(&remote_spec);
        cmd.arg(local_path);
        let output = cmd
            .output()
            .map_err(|e| ConfigError::Remote(format!("scp failed: {e}")))?;
        if output.status.success() {
            return Ok(());
        }
        let scp_err = format!(
            "scp failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        self.fetch_via_ssh_cat(remote_path, local_path)
            .map_err(|ssh_err| ConfigError::Remote(format!("{scp_err}; ssh fallback: {ssh_err}")))
    }

    /// Fallback when `scp` auth fails but `ssh` works (same keys, different subsystem).
    fn fetch_via_ssh_cat(&self, remote_path: &str, local_path: &Path) -> Result<(), ConfigError> {
        let path = self.resolve_path_on_host(remote_path)?;
        let escaped = shell_escape_path(&path);
        let output = self.run_remote(&format!("cat {escaped}"))?;
        if !output.status.success() {
            return Err(ConfigError::Remote(format!(
                "ssh cat failed ({}): {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        std::fs::write(local_path, &output.stdout)
            .map_err(|e| ConfigError::Remote(format!("write cache file: {e}")))?;
        Ok(())
    }

    fn base_scp_args(&self) -> Vec<String> {
        let mut args = vec![
            "-o".into(),
            "BatchMode=yes".into(),
            "-o".into(),
            "StrictHostKeyChecking=accept-new".into(),
            "-P".into(),
            self.port.to_string(),
        ];
        if let Some(identity) = &self.identity_file {
            args.push("-i".into());
            args.push(identity.to_string_lossy().into_owned());
        }
        args
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMeta {
    pub config_fetched_at: u64,
    pub database_fetched_at: u64,
}

impl CacheMeta {
    pub fn load(cache_dir: &Path) -> Option<Self> {
        let path = cache_dir.join(".meta.json");
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn save(&self, cache_dir: &Path) -> Result<(), ConfigError> {
        std::fs::create_dir_all(cache_dir)?;
        let path = cache_dir.join(".meta.json");
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

pub fn cache_is_fresh(fetched_at: u64, ttl_secs: u64) -> bool {
    if ttl_secs == 0 {
        return false;
    }
    now_secs().saturating_sub(fetched_at) < ttl_secs
}

pub fn default_cache_dir(host: &str) -> PathBuf {
    expand_tilde(&format!("~/.cache/void/remote/{host}"))
}

pub fn fetch_remote_file(
    ssh: &SshTarget,
    remote_path: &str,
    local_path: &Path,
) -> Result<(), ConfigError> {
    ssh.scp_from(remote_path, local_path)
}

pub fn fetch_remote_files_if_present(
    ssh: &SshTarget,
    remote_dir: &str,
    filenames: &[&str],
    cache_dir: &Path,
) -> Result<(), ConfigError> {
    std::fs::create_dir_all(cache_dir)?;
    for name in filenames {
        let remote_path = format!("{remote_dir}/{name}");
        let local_path = cache_dir.join(name);
        if *name == "void.db" {
            fetch_remote_file(ssh, &remote_path, &local_path)?;
        } else if ssh.scp_from(&remote_path, &local_path).is_err() {
            // WAL sidecars may not exist yet on a quiet database.
            let _ = std::fs::remove_file(&local_path);
        }
    }
    Ok(())
}

fn shell_escape_path(path: &str) -> String {
    if path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '@'))
    {
        path.to_string()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}

/// Format a remote path for scp. The remote login shell parses this string, so
/// every metacharacter must be quoted to prevent command injection. A leading
/// `~/` is kept unquoted so `$HOME` still expands; only the remainder (which is
/// where untrusted/config-sourced data lives) is escaped.
fn format_remote_scp_path(remote_path: &str) -> String {
    if let Some(rest) = remote_path.strip_prefix("~/") {
        format!("~/{}", shell_escape_path(rest))
    } else if remote_path == "~" {
        "~".to_string()
    } else {
        shell_escape_path(remote_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scp_path_keeps_tilde_unquoted() {
        assert_eq!(
            format_remote_scp_path("~/.config/void/config.toml"),
            "~/.config/void/config.toml"
        );
    }

    #[test]
    fn scp_path_quotes_spaces() {
        assert_eq!(
            format_remote_scp_path("/path/with spaces/file"),
            "'/path/with spaces/file'"
        );
    }

    #[test]
    fn scp_path_keeps_tilde_for_upload_destination() {
        assert_eq!(
            format_remote_scp_path("~/.local/share/void/staging/uuid-file.pdf"),
            "~/.local/share/void/staging/uuid-file.pdf"
        );
    }
}

// Fake ssh/scp integration tests. `run_remote`, `scp_to` and `scp_from` shell
// out to PATH-resolved `ssh`/`scp`, so we can drop fake scripts into a tempdir,
// prepend it to PATH, and observe argv + error surfacing. PATH is process-global,
// so these tests serialize on a shared mutex.
#[cfg(unix)]
#[cfg(test)]
mod fake_ssh_tests {
    use super::*;

    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;

    // Serializes PATH mutation across tests in this module.
    static PATH_GUARD: Mutex<()> = Mutex::new(());

    fn write_script(dir: &std::path::Path, name: &str, body: &str) {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        f.flush().unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
    }

    /// Run `f` with `dir` prepended to PATH, restoring PATH afterwards.
    fn with_path_prefix<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
        let _guard = PATH_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let original = std::env::var_os("PATH");
        let mut new_path = std::ffi::OsString::from(dir);
        if let Some(orig) = &original {
            new_path.push(":");
            new_path.push(orig);
        }
        std::env::set_var("PATH", &new_path);
        let result = f();
        match original {
            Some(p) => std::env::set_var("PATH", p),
            None => std::env::remove_var("PATH"),
        }
        result
    }

    fn target(port: u16) -> SshTarget {
        SshTarget {
            host: "fakehost".into(),
            user: Some("bob".into()),
            port,
            identity_file: None,
        }
    }

    #[test]
    fn run_remote_argv_includes_destination_and_command() {
        let dir = tempfile::tempdir().unwrap();
        let argv_log = dir.path().join("ssh-argv.txt");
        // Fake ssh records its argv (one per line) and prints fixed stdout.
        write_script(
            dir.path(),
            "ssh",
            &format!(
                "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{}'\necho REMOTE_OK\nexit 0\n",
                argv_log.display()
            ),
        );

        let out = with_path_prefix(dir.path(), || target(2222).run_remote("echo hi").unwrap());
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "REMOTE_OK");

        let logged = std::fs::read_to_string(&argv_log).unwrap();
        let args: Vec<&str> = logged.lines().collect();
        // base_ssh_args: -o BatchMode=yes -o StrictHostKeyChecking=accept-new -p <port>
        assert!(args.contains(&"BatchMode=yes"), "argv: {args:?}");
        assert!(
            args.contains(&"StrictHostKeyChecking=accept-new"),
            "argv: {args:?}"
        );
        let port_idx = args.iter().position(|a| *a == "-p").unwrap();
        assert_eq!(args[port_idx + 1], "2222", "port forwarded as -p value");
        assert!(args.contains(&"bob@fakehost"), "destination present");
        assert!(args.contains(&"echo hi"), "remote command is last arg");
    }

    #[test]
    fn run_remote_surfaces_nonzero_exit_via_output_status() {
        let dir = tempfile::tempdir().unwrap();
        write_script(
            dir.path(),
            "ssh",
            "#!/bin/sh\necho 'connection refused' 1>&2\nexit 255\n",
        );

        let out = with_path_prefix(dir.path(), || target(22).run_remote("whoami").unwrap());
        assert!(!out.status.success(), "non-zero exit propagated");
        assert!(String::from_utf8_lossy(&out.stderr).contains("connection refused"));
    }

    #[test]
    fn scp_to_argv_orders_local_then_remote_and_uses_capital_p_port() {
        let dir = tempfile::tempdir().unwrap();
        let argv_log = dir.path().join("scp-argv.txt");
        write_script(
            dir.path(),
            "scp",
            &format!(
                "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{}'\nexit 0\n",
                argv_log.display()
            ),
        );

        let local = dir.path().join("payload.bin");
        std::fs::write(&local, b"data").unwrap();

        with_path_prefix(dir.path(), || {
            target(2200)
                .scp_to(&local, "/remote/dir/payload.bin")
                .unwrap()
        });

        let logged = std::fs::read_to_string(&argv_log).unwrap();
        let args: Vec<&str> = logged.lines().collect();
        // scp uses -P (capital) for port, unlike ssh's -p.
        let p_idx = args.iter().position(|a| *a == "-P").unwrap();
        assert_eq!(args[p_idx + 1], "2200");
        // Last two positionals: local source, then remote dest.
        let last = args.last().unwrap();
        let second_last = &args[args.len() - 2];
        assert_eq!(*second_last, local.to_string_lossy());
        assert_eq!(*last, "bob@fakehost:/remote/dir/payload.bin");
    }

    #[test]
    fn scp_to_surfaces_error_when_fake_exits_nonzero() {
        let dir = tempfile::tempdir().unwrap();
        write_script(
            dir.path(),
            "scp",
            "#!/bin/sh\necho 'permission denied' 1>&2\nexit 1\n",
        );
        let local = dir.path().join("f.bin");
        std::fs::write(&local, b"x").unwrap();

        let err = with_path_prefix(dir.path(), || {
            target(22).scp_to(&local, "/remote/f.bin").unwrap_err()
        });
        let msg = err.to_string();
        assert!(msg.contains("scp failed"), "msg: {msg}");
        assert!(msg.contains("permission denied"), "stderr surfaced: {msg}");
    }

    #[test]
    fn execute_proxy_uploads_runs_mkdir_then_scp_in_order() {
        use crate::store::proxy_files::{execute_proxy_uploads, StagedUpload};

        let dir = tempfile::tempdir().unwrap();
        let order_log = dir.path().join("order.txt");
        // ssh handles the mkdir -p staging step; record that it ran.
        write_script(
            dir.path(),
            "ssh",
            &format!(
                "#!/bin/sh\nprintf 'ssh\\n' >> '{}'\nexit 0\n",
                order_log.display()
            ),
        );
        write_script(
            dir.path(),
            "scp",
            &format!(
                "#!/bin/sh\nprintf 'scp\\n' >> '{}'\nexit 0\n",
                order_log.display()
            ),
        );

        let local = dir.path().join("attach.pdf");
        std::fs::write(&local, b"pdf").unwrap();
        let uploads = vec![StagedUpload {
            local_path: local,
            remote_path: "/store/staging/x-attach.pdf".into(),
        }];

        with_path_prefix(dir.path(), || {
            execute_proxy_uploads(&target(22), "/store", &uploads).unwrap();
        });

        let order = std::fs::read_to_string(&order_log).unwrap();
        let steps: Vec<&str> = order.lines().collect();
        // ensure_remote_dir (ssh mkdir) must precede the scp upload.
        assert_eq!(steps.first(), Some(&"ssh"), "mkdir before scp: {steps:?}");
        assert!(steps.contains(&"scp"), "scp upload ran: {steps:?}");
        let ssh_pos = steps.iter().position(|s| *s == "ssh").unwrap();
        let scp_pos = steps.iter().position(|s| *s == "scp").unwrap();
        assert!(ssh_pos < scp_pos, "staging mkdir precedes scp: {steps:?}");
    }
}

#[cfg(test)]
mod ssh_tests {
    use super::*;

    #[test]
    fn ssh_destination_with_user() {
        let target = SshTarget {
            host: "homeserver".into(),
            user: Some("alice".into()),
            port: 22,
            identity_file: None,
        };
        assert_eq!(target.destination(), "alice@homeserver");
    }

    #[test]
    fn cache_freshness_respects_ttl() {
        let now = now_secs();
        assert!(cache_is_fresh(now, 30));
        assert!(!cache_is_fresh(now.saturating_sub(60), 30));
        assert!(!cache_is_fresh(now, 0));
    }
}
