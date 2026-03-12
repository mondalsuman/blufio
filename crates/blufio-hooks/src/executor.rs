// SPDX-FileCopyrightText: 2026 Blufio Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Shell executor for hook commands.
//!
//! Runs shell commands with JSON stdin, stdout capture, configurable timeout,
//! and restricted PATH for security. Commands execute via `sh -c` with a
//! cleared environment (only `PATH` and `HOME` are set).

use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Errors that can occur during hook command execution.
#[derive(Debug, thiserror::Error)]
pub enum HookError {
    /// Failed to spawn the child process.
    #[error("failed to spawn hook process: {0}")]
    SpawnFailed(std::io::Error),

    /// The hook command exceeded its timeout.
    #[error("hook timed out after {0:?}")]
    Timeout(Duration),

    /// The hook command exited with a non-zero status.
    #[error("hook exited with status {0:?}")]
    NonZeroExit(Option<i32>),

    /// An I/O error occurred during hook execution.
    #[error("I/O error during hook execution: {0}")]
    IoError(#[from] std::io::Error),

    /// Recursion limit exceeded (hook would trigger itself).
    #[error("recursion limit exceeded")]
    RecursionLimitExceeded,
}

/// Result of a successful hook execution.
#[derive(Debug)]
pub struct HookResult {
    /// Captured stdout (None if empty).
    pub stdout: Option<String>,
    /// Captured stderr.
    pub stderr: String,
    /// Process exit code.
    pub exit_code: i32,
    /// Execution duration.
    pub duration: Duration,
}

/// Execute a hook command in a restricted shell environment.
///
/// The command runs via `sh -c` with:
/// - Environment cleared except for `PATH` and `HOME=/tmp`
/// - JSON written to the process stdin
/// - stdout and stderr captured
/// - A configurable timeout that kills the process if exceeded
///
/// # Errors
///
/// Returns [`HookError::SpawnFailed`] if the process cannot be started,
/// [`HookError::Timeout`] if execution exceeds the deadline,
/// [`HookError::NonZeroExit`] if the command exits with non-zero status,
/// or [`HookError::IoError`] for other I/O failures.
pub async fn execute_hook(
    command: &str,
    stdin_json: &str,
    timeout: Duration,
    allowed_path: &str,
) -> Result<HookResult, HookError> {
    let start = Instant::now();

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .env_clear()
        .env("PATH", allowed_path)
        .env("HOME", "/tmp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(HookError::SpawnFailed)?;

    // Write JSON to stdin, then close.
    if let Some(mut stdin_handle) = child.stdin.take() {
        // Ignore write errors (process may have already exited).
        let _ = stdin_handle.write_all(stdin_json.as_bytes()).await;
        drop(stdin_handle);
    }

    // Take stdout/stderr handles for manual reading.
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    // Wait with timeout. Use child.wait() (borrows) instead of
    // wait_with_output() (takes ownership) so we can kill on timeout.
    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(result) => result?,
        Err(_) => {
            // Timeout: kill the child process.
            let _ = child.kill().await;
            return Err(HookError::Timeout(timeout));
        }
    };

    let duration = start.elapsed();

    // Read captured output after process exits.
    let stdout_bytes = if let Some(mut h) = stdout_handle {
        let mut buf = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut h, &mut buf).await?;
        buf
    } else {
        Vec::new()
    };

    let stderr_bytes = if let Some(mut h) = stderr_handle {
        let mut buf = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut h, &mut buf).await?;
        buf
    } else {
        Vec::new()
    };

    let exit_code = status.code().unwrap_or(-1);
    let stdout_str = String::from_utf8_lossy(&stdout_bytes).to_string();
    let stderr_str = String::from_utf8_lossy(&stderr_bytes).to_string();

    if !status.success() {
        return Err(HookError::NonZeroExit(status.code()));
    }

    Ok(HookResult {
        stdout: if stdout_str.is_empty() {
            None
        } else {
            Some(stdout_str)
        },
        stderr: stderr_str,
        exit_code,
        duration,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn make_script(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();

        // Make executable.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(f.path(), perms).unwrap();
        }

        f
    }

    #[tokio::test]
    async fn execute_simple_echo() {
        let result = execute_hook(
            "echo hello",
            "{}",
            Duration::from_secs(5),
            "/usr/bin:/usr/local/bin:/bin",
        )
        .await
        .unwrap();

        assert_eq!(result.stdout.as_deref(), Some("hello\n"));
        assert_eq!(result.exit_code, 0);
        assert!(result.duration < Duration::from_secs(5));
    }

    #[tokio::test]
    async fn json_stdin_received_by_script() {
        let script = make_script("#!/bin/sh\ncat\n");
        let cmd = format!("sh {}", script.path().display());

        let result = execute_hook(
            &cmd,
            r#"{"event":"session.created","id":"123"}"#,
            Duration::from_secs(5),
            "/usr/bin:/usr/local/bin:/bin",
        )
        .await
        .unwrap();

        let stdout = result.stdout.unwrap();
        assert!(stdout.contains("session.created"));
        assert!(stdout.contains("123"));
    }

    #[tokio::test]
    async fn timeout_kills_long_running_process() {
        let result = execute_hook(
            "sleep 60",
            "{}",
            Duration::from_millis(200),
            "/usr/bin:/usr/local/bin:/bin",
        )
        .await;

        match result {
            Err(HookError::Timeout(d)) => {
                assert_eq!(d, Duration::from_millis(200));
            }
            other => panic!("expected Timeout, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn non_zero_exit_returns_error() {
        let result = execute_hook(
            "exit 42",
            "{}",
            Duration::from_secs(5),
            "/usr/bin:/usr/local/bin:/bin",
        )
        .await;

        match result {
            Err(HookError::NonZeroExit(code)) => {
                assert_eq!(code, Some(42));
            }
            other => panic!("expected NonZeroExit, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn env_clear_restricts_path() {
        // With a restricted PATH that doesn't include common directories,
        // commands should fail to find executables.
        let result = execute_hook(
            "python3 --version",
            "{}",
            Duration::from_secs(5),
            "/nonexistent/path",
        )
        .await;

        // Should fail because python3 isn't in /nonexistent/path.
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn empty_stdout_returns_none() {
        // `true` produces no output.
        let result = execute_hook(
            "true",
            "{}",
            Duration::from_secs(5),
            "/usr/bin:/usr/local/bin:/bin",
        )
        .await
        .unwrap();

        assert!(result.stdout.is_none());
        assert_eq!(result.exit_code, 0);
    }
}
