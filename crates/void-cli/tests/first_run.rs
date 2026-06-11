//! First-run behavior: a fresh machine with no store and no config must not
//! panic. We assert clean exits (success or a graceful error), never a Rust
//! panic / SIGABRT.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn void() -> Command {
    Command::cargo_bin("void").expect("void binary should be built")
}

/// On unix, a panic aborts/handler-exits with a signal or code 101. Assert the
/// process exited "normally" (with a real exit code, no panic backtrace).
fn assert_no_panic(output: &std::process::Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("panicked at"),
        "process panicked:\n{stderr}"
    );
    assert!(
        !stderr.contains("RUST_BACKTRACE"),
        "process printed a panic backtrace hint:\n{stderr}"
    );
    // Code 101 is the conventional Rust panic exit code.
    if let Some(code) = output.status.code() {
        assert_ne!(
            code, 101,
            "process exited with the panic code 101:\n{stderr}"
        );
    } else {
        // No exit code => killed by a signal (e.g. SIGABRT from a panic=abort).
        panic!("process was killed by a signal:\n{stderr}");
    }
}

#[test]
fn inbox_on_fresh_machine_does_not_panic() {
    // Fresh empty store dir + a config path that does NOT exist yet. The CLI
    // auto-creates a default config and an empty db; `--store` keeps everything
    // inside the tempdir.
    let dir = tempfile::tempdir().expect("tempdir");
    let store = dir.path().join("store");
    std::fs::create_dir_all(&store).expect("create store dir");
    let config = dir.path().join("does-not-exist.toml");
    assert!(!config.exists());

    let output = void()
        .arg("--store")
        .arg(&store)
        .arg("--config")
        .arg(&config)
        .arg("inbox")
        .output()
        .expect("run inbox");

    assert_no_panic(&output);
    // Empty store yields a clean empty result set, exit 0.
    assert!(
        output.status.success(),
        "inbox on empty store should exit 0, got {:?}",
        output.status.code()
    );
}

#[test]
fn doctor_non_interactive_exits_with_defined_code() {
    // Pin store.path to the tempdir so doctor's config reload + db open stay
    // isolated from real user data.
    let dir: TempDir = tempfile::tempdir().expect("tempdir");
    let store = dir.path().join("store");
    std::fs::create_dir_all(&store).expect("create store dir");
    // Escape backslashes so a Windows path (C:\...) is a valid TOML basic string.
    let store_str = store.to_string_lossy().replace('\\', "\\\\");
    let config = dir.path().join("config.toml");
    std::fs::write(
        &config,
        format!("[store]\nmode = \"local\"\npath = \"{store_str}\"\n"),
    )
    .expect("write config");

    let output = void()
        .arg("--store")
        .arg(&store)
        .arg("--config")
        .arg(&config)
        .args(["doctor", "--non-interactive"])
        .output()
        .expect("run doctor");

    assert_no_panic(&output);
    // doctor returns a defined exit code: 0 (healthy) or 1 (issues found, e.g.
    // no connections configured). Anything else would be a crash.
    let code = output
        .status
        .code()
        .expect("doctor should have an exit code");
    assert!(
        code == 0 || code == 1,
        "doctor exit code should be 0 or 1, got {code}"
    );
    // It should produce its health report on stderr.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        predicate::str::contains("void doctor").eval(&stderr),
        "doctor should print its health report header, stderr:\n{stderr}"
    );
}
