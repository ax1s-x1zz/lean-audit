//! Subprocess execution with a wall-clock timeout.
//!
//! `lake build` and `lake env lean` can take minutes and can hang. The auditor
//! must never hang with them, so:
//!
//! - stdout and stderr are drained on separate threads (a chatty build must not
//!   deadlock on a full pipe);
//! - the child is killed when the wall-clock budget runs out;
//! - the pipes are then awaited with a short grace period rather than joined, so
//!   a grandchild that inherited the pipe cannot stall the auditor.
//!
//! Output is deliberately lost when the grace period expires: on a timeout the
//! auditor reports a timeout, and partial build output is not worth hanging for.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long to wait for the pipes after the process has gone.
const PIPE_GRACE: Duration = Duration::from_secs(5);

pub struct RunOutput {
    pub returncode: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    /// Wall-clock time until the process exited (or was killed), excluding the
    /// pipe grace period.
    pub wall_ms: u64,
    /// True when output had to be abandoned because a pipe stayed open.
    pub output_abandoned: bool,
}

impl RunOutput {
    /// stdout and stderr concatenated, which is how Lean reports diagnostics.
    pub fn combined(&self) -> String {
        let mut s = self.stdout.clone();
        if !self.stderr.is_empty() {
            if !s.is_empty() && !s.ends_with('\n') {
                s.push('\n');
            }
            s.push_str(&self.stderr);
        }
        s
    }

    pub fn tail(&self, max_chars: usize) -> String {
        let combined = self.combined();
        let trimmed = combined.trim_end();
        if trimmed.chars().count() <= max_chars {
            return trimmed.to_string();
        }
        let skip = trimmed.chars().count() - max_chars;
        format!("…{}", trimmed.chars().skip(skip).collect::<String>())
    }
}

pub fn run(
    program: &str,
    args: &[String],
    cwd: &Path,
    timeout: Duration,
) -> std::io::Result<RunOutput> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let (tx_out, rx_out) = mpsc::channel();
    let (tx_err, rx_err) = mpsc::channel();
    if let Some(mut pipe) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = pipe.read_to_string(&mut s);
            let _ = tx_out.send(s);
        });
    }
    if let Some(mut pipe) = child.stderr.take() {
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = pipe.read_to_string(&mut s);
            let _ = tx_err.send(s);
        });
    }

    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait()? {
            Some(status) => break Some(status),
            None => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    timed_out = true;
                    break child.wait().ok();
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    };
    // Measured before the pipe grace period, so a lingering grandchild does not
    // inflate the reported runtime.
    let wall_ms = start.elapsed().as_millis() as u64;

    let stdout = rx_out.recv_timeout(PIPE_GRACE);
    let stderr = rx_err.recv_timeout(PIPE_GRACE);
    let output_abandoned = stdout.is_err() || stderr.is_err();

    Ok(RunOutput {
        returncode: status.and_then(|s| s.code()),
        stdout: stdout.unwrap_or_default(),
        stderr: stderr.unwrap_or_default(),
        timed_out,
        wall_ms,
        output_abandoned,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_stdout_and_exit_code() {
        let out = run(
            "sh",
            &["-c".into(), "echo hi".into()],
            Path::new("/"),
            Duration::from_secs(10),
        )
        .expect("spawn sh");
        assert_eq!(out.returncode, Some(0));
        assert_eq!(out.stdout.trim(), "hi");
        assert!(!out.timed_out);
        assert!(!out.output_abandoned);
    }

    #[test]
    fn captures_stderr_and_nonzero_exit() {
        let out = run(
            "sh",
            &["-c".into(), "echo oops >&2; exit 7".into()],
            Path::new("/"),
            Duration::from_secs(10),
        )
        .expect("spawn sh");
        assert_eq!(out.returncode, Some(7));
        assert!(out.combined().contains("oops"));
    }

    #[test]
    fn kills_a_hung_process() {
        let out = run(
            "sh",
            &["-c".into(), "sleep 30".into()],
            Path::new("/"),
            Duration::from_millis(300),
        )
        .expect("spawn sh");
        assert!(out.timed_out);
        assert!(
            out.wall_ms < 10_000,
            "must not wait for the full sleep, got {}ms",
            out.wall_ms
        );
    }

    #[test]
    fn a_grandchild_holding_the_pipe_does_not_stall_the_caller() {
        // `sh` exits immediately but the background `sleep` inherits the pipes.
        let start = Instant::now();
        let out = run(
            "sh",
            &["-c".into(), "sleep 30 & exit 0".into()],
            Path::new("/"),
            Duration::from_secs(10),
        )
        .expect("spawn sh");
        assert_eq!(out.returncode, Some(0));
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "the pipe grace period must bound the wait"
        );
        assert!(
            out.output_abandoned,
            "output is abandoned, and reported as such"
        );
    }

    #[test]
    fn missing_program_is_an_io_error() {
        assert!(
            run(
                "definitely-not-a-real-binary-xyz",
                &[],
                Path::new("/"),
                Duration::from_secs(1)
            )
            .is_err()
        );
    }

    #[test]
    fn tail_keeps_the_end() {
        let out = RunOutput {
            returncode: Some(0),
            stdout: "a".repeat(100) + "TAIL",
            stderr: String::new(),
            timed_out: false,
            wall_ms: 0,
            output_abandoned: false,
        };
        let t = out.tail(10);
        assert!(t.ends_with("TAIL"));
        assert!(t.chars().count() <= 11);
    }
}
