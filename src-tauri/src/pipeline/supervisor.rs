//! Consented process supervisor (ROADMAP R1-1)
//!
//! Owns a single child process placed in its own process group (`setsid`) so
//! cleanup can only ever reach our own children — never an unrelated listener
//! or PID. Enforces a wall-clock deadline with SIGTERM → SIGKILL escalation,
//! bounds captured output, and returns an `ExitStatus` proof so callers can
//! distinguish a real termination from a timed-out or killed run.
//!
//! This is the substrate the supervised verifiers (R1-2) and the consented
//! installer (F07) build on. It never shells out through a user-controllable
//! string; the program and args are taken verbatim.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Scoped consent to run a supervised subprocess for a verification gate.
///
/// Only `Granted` may spawn a child. `NotSupplied` (the default) and `Denied`
/// are treated identically by the verifiers: the gate is reported `Unknown`,
/// never a pass — an unconsented or missing consent must not be mistaken for
/// a verified build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExecutionConsent {
    /// No consent was supplied for this run.
    #[default]
    NotSupplied,
    /// Explicitly granted: the allowlisted verifier subprocess may run.
    Granted,
    /// Explicitly withheld by the user.
    Denied,
}

impl ExecutionConsent {
    /// True only when a supervised subprocess is permitted to run.
    pub fn is_granted(self) -> bool {
        matches!(self, ExecutionConsent::Granted)
    }
}

/// Bounds a supervised child: wall deadline, SIGTERM grace, and output cap.
#[derive(Debug, Clone, Copy)]
pub struct SupervisorLimits {
    /// Hard wall-clock deadline for the child's lifetime.
    pub timeout: Duration,
    /// Grace period after SIGTERM before escalating to SIGKILL.
    pub term_grace: Duration,
    /// Maximum bytes retained per output stream.
    pub max_output_bytes: usize,
}

impl Default for SupervisorLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(180),
            term_grace: Duration::from_secs(5),
            max_output_bytes: 256 * 1024,
        }
    }
}

/// How a supervised child came to an end. Only `Exited` carries a real exit
/// code; the rest are proof the child was stopped by the supervisor.
#[derive(Debug, Clone, PartialEq)]
pub enum Termination {
    Exited(i32),
    TimedOut,
    /// The caller requested cancellation while the child was in flight; the
    /// supervisor stopped it with the same bounded TERM-then-KILL escalation
    /// used for timeouts.
    Cancelled,
    FailedToStart(String),
}

/// Structured receipt for a supervised run. An in-process check cannot forge
/// one: `command` is the exact argv, `duration_ms` is measured, and
/// `termination` distinguishes a real exit from supervisor intervention.
#[derive(Debug, Clone)]
pub struct SupervisorReceipt {
    pub command: Vec<String>,
    pub termination: Termination,
    pub duration_ms: u64,
    pub output: String,
    /// True only when the child exited by itself within the deadline.
    pub clean_exit: bool,
}

/// Result of a supervised run: the receipt plus the raw `ExitStatus` when the
/// child exited (None when it was killed or timed out).
#[derive(Debug)]
pub struct SupervisedRun {
    pub receipt: SupervisorReceipt,
    pub status: Option<std::process::ExitStatus>,
}

/// Spawn a child in its own process group and supervise it to a terminal
/// state under `limits`. Returns a receipt and, when the child exited, its
/// `ExitStatus`.
///
/// The child runs in a fresh session (new process group id == its own pid),
/// so the group-signal below can only reach descendants of this child.
pub fn run_supervised(
    program: &str,
    args: &[&str],
    cwd: &Path,
    limits: &SupervisorLimits,
) -> SupervisedRun {
    let started = Instant::now();
    let command_argv: Vec<String> = std::iter::once(program.to_string())
        .chain(args.iter().map(|a| a.to_string()))
        .collect();

    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("NO_COLOR", "1")
        .env("TERM", "dumb");
    // New session so the child owns a process group == its own pid, letting
    // cleanup signal the whole tree and nothing else. The closure must only
    // call async-signal-safe operations before exec.
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            rustix::process::setsid()?;
            Ok(())
        });
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return SupervisedRun {
                receipt: SupervisorReceipt {
                    command: command_argv,
                    termination: Termination::FailedToStart(error.to_string()),
                    duration_ms: started.elapsed().as_millis() as u64,
                    output: String::new(),
                    clean_exit: false,
                },
                status: None,
            };
        }
    };

    supervise_child(child, command_argv, limits, started, None)
}

/// Cancellable variant of [`run_supervised`]: `should_cancel` is polled on the
/// same 25ms cadence as the wall deadline; a `true` stops the child with the
/// bounded TERM-then-KILL escalation and reports `Termination::Cancelled`.
pub fn run_supervised_cancellable(
    program: &str,
    args: &[&str],
    cwd: &Path,
    limits: &SupervisorLimits,
    should_cancel: &dyn Fn() -> bool,
) -> SupervisedRun {
    let started = Instant::now();
    let command_argv: Vec<String> = std::iter::once(program.to_string())
        .chain(args.iter().map(|a| a.to_string()))
        .collect();

    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("NO_COLOR", "1")
        .env("TERM", "dumb");
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            rustix::process::setsid()?;
            Ok(())
        });
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return SupervisedRun {
                receipt: SupervisorReceipt {
                    command: command_argv,
                    termination: Termination::FailedToStart(error.to_string()),
                    duration_ms: started.elapsed().as_millis() as u64,
                    output: String::new(),
                    clean_exit: false,
                },
                status: None,
            };
        }
    };

    supervise_child(child, command_argv, limits, started, Some(should_cancel))
}

fn supervise_child(
    mut child: std::process::Child,
    command_argv: Vec<String>,
    limits: &SupervisorLimits,
    started: Instant,
    should_cancel: Option<&dyn Fn() -> bool>,
) -> SupervisedRun {
    let pid = child.id();

    // Reap stdout/stderr on *two* background threads so a child that fills
    // one pipe cannot deadlock while we drain the other. Each stream is
    // bounded to `max_output_bytes`.
    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();
    let max_bytes = limits.max_output_bytes;
    let mut out_reader = stdout_handle.map(|handle| {
        thread::spawn(move || {
            let mut collected = Vec::new();
            read_bounded(handle, &mut collected, max_bytes);
            String::from_utf8_lossy(&collected).into_owned()
        })
    });
    let mut err_reader = stderr_handle.map(|handle| {
        thread::spawn(move || {
            let mut collected = Vec::new();
            read_bounded(handle, &mut collected, max_bytes);
            String::from_utf8_lossy(&collected).into_owned()
        })
    });

    let deadline = started + limits.timeout;
    let mut terminated = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                terminated = true;
                let output = drain_readers(&mut out_reader, &mut err_reader);
                return SupervisedRun {
                    receipt: SupervisorReceipt {
                        command: command_argv,
                        termination: Termination::Exited(status.code().unwrap_or(-1)),
                        duration_ms: started.elapsed().as_millis() as u64,
                        output,
                        clean_exit: true,
                    },
                    status: Some(status),
                };
            }
            Ok(None) => {}
            Err(_) => {
                // Reaping error; treat as gone and clean up below.
                break;
            }
        }
        if should_cancel.is_some_and(|check| check()) {
            break; // caller requested cancellation
        }
        if Instant::now() >= deadline {
            break; // hard deadline reached
        }
        thread::sleep(Duration::from_millis(25));
    }

    let timed_out = Instant::now() >= deadline;
    // Timed out or cancelled: SIGTERM the whole group, wait for the grace
    // period, then SIGKILL anything still alive.
    signal_group(pid, rustix::process::Signal::TERM);
    let grace_deadline = Instant::now() + limits.term_grace;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                terminated = true;
                let output = drain_readers(&mut out_reader, &mut err_reader);
                return SupervisedRun {
                    receipt: SupervisorReceipt {
                        command: command_argv,
                        termination: if timed_out {
                            Termination::TimedOut
                        } else {
                            Termination::Cancelled
                        },
                        duration_ms: started.elapsed().as_millis() as u64,
                        output,
                        clean_exit: false,
                    },
                    status: Some(status),
                };
            }
            Ok(None) => {}
            Err(_) => break,
        }
        if Instant::now() >= grace_deadline {
            signal_group(pid, rustix::process::Signal::KILL);
            let _ = child.wait();
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    if !terminated {
        // Final best-effort: kill the group and wait so we never leak a child.
        signal_group(pid, rustix::process::Signal::KILL);
        let _ = child.wait();
    }

    let output = drain_readers(&mut out_reader, &mut err_reader);
    SupervisedRun {
        receipt: SupervisorReceipt {
            command: command_argv,
            termination: if timed_out {
                Termination::TimedOut
            } else {
                Termination::Cancelled
            },
            duration_ms: started.elapsed().as_millis() as u64,
            output,
            clean_exit: false,
        },
        status: None,
    }
}

/// Join both output reader threads, combining stdout then stderr. Takes
/// `&mut` and consumes each handle via `.take()` so the joined `String` is
/// owned, not borrowed.
fn drain_readers(
    out: &mut Option<thread::JoinHandle<String>>,
    err: &mut Option<thread::JoinHandle<String>>,
) -> String {
    let mut combined = String::new();
    if let Some(handle) = out.take() {
        combined.push_str(&handle.join().unwrap_or_default());
    }
    if let Some(handle) = err.take() {
        if !combined.is_empty() {
            combined.push('\n');
        }
        combined.push_str(&handle.join().unwrap_or_default());
    }
    combined
}

/// Signal the child's process group (pgid == child pid). Failures are
/// tolerated: the child may already be gone, and we never signal an unrelated
/// pid. `from_raw` returns None for the pid-0 sentinel.
fn signal_group(pid: u32, signal: rustix::process::Signal) {
    if let Some(group) = rustix::process::Pid::from_raw(pid as i32) {
        let _ = rustix::process::kill_process_group(group, signal);
    }
}

/// Read from a pipe to EOF, retaining at most `budget` bytes. Beyond the
/// budget the pipe is still drained so the child never blocks, but the excess
/// is dropped. Takes the reader by value (it is owned by the calling thread).
fn read_bounded<R: Read>(mut reader: R, sink: &mut Vec<u8>, budget: usize) {
    let mut buf = [0u8; 8192];
    loop {
        let allow = budget.saturating_sub(sink.len()).min(buf.len());
        if allow == 0 {
            // At budget: drain without retaining so the child can finish.
            // Must still break on EOF, otherwise after the child is killed the
            // read end sees EOF (Ok(0)) and this branch spins forever in
            // userspace, blocking the join.
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            continue;
        }
        match reader.read(&mut buf[..allow]) {
            Ok(0) => break,
            Ok(n) => sink.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_sh(cmd: &str, limits: &SupervisorLimits) -> SupervisedRun {
        run_supervised(
            "/bin/sh",
            &["-c", cmd],
            std::env::temp_dir().as_path(),
            limits,
        )
    }

    fn default_limits() -> SupervisorLimits {
        SupervisorLimits {
            timeout: Duration::from_secs(10),
            ..Default::default()
        }
    }

    #[test]
    fn consent_only_granted_permits_execution() {
        assert!(ExecutionConsent::Granted.is_granted());
        assert!(!ExecutionConsent::NotSupplied.is_granted());
        assert!(!ExecutionConsent::Denied.is_granted());
    }

    #[test]
    fn clean_exit_reports_code_and_stdout() {
        let run = run_sh("echo hello-stdout", &default_limits());
        assert!(run.receipt.clean_exit);
        assert_eq!(run.receipt.termination, Termination::Exited(0));
        assert!(run.receipt.output.contains("hello-stdout"));
        assert!(run.status.is_some());
    }

    #[test]
    fn nonzero_exit_reports_its_code() {
        let run = run_sh("echo boom; exit 3", &default_limits());
        assert_eq!(run.receipt.termination, Termination::Exited(3));
        assert!(run.receipt.output.contains("boom"));
    }

    #[test]
    fn captures_stderr_separately_from_stdout() {
        let run = run_sh("echo out-line; echo err-line 1>&2", &default_limits());
        assert!(run.receipt.output.contains("out-line"));
        assert!(run.receipt.output.contains("err-line"));
    }

    #[test]
    fn output_is_bounded_to_budget() {
        let limits = SupervisorLimits {
            timeout: Duration::from_secs(10),
            max_output_bytes: 1024,
            ..Default::default()
        };
        // yes prints unbounded; capture must be capped per stream.
        let run = run_sh("yes", &limits);
        assert!(
            run.receipt.output.len() <= 2 * 1024 + 4,
            "captured {} bytes",
            run.receipt.output.len()
        );
    }

    #[test]
    fn timeout_escalates_and_reports_not_clean() {
        let limits = SupervisorLimits {
            timeout: Duration::from_millis(150),
            term_grace: Duration::from_millis(100),
            ..Default::default()
        };
        let started = Instant::now();
        let run = run_sh("sleep 30", &limits);
        assert!(!run.receipt.clean_exit);
        assert!(matches!(run.receipt.termination, Termination::TimedOut));
        assert!(started.elapsed() >= Duration::from_millis(100));
        assert!(started.elapsed() < Duration::from_secs(30));
    }

    #[test]
    fn failed_start_reports_reason_without_panic() {
        let limits = SupervisorLimits::default();
        let run = run_supervised(
            "/nonexistent/binary/xyz",
            &["--flag"],
            std::env::temp_dir().as_path(),
            &limits,
        );
        assert!(matches!(
            run.receipt.termination,
            Termination::FailedToStart(_)
        ));
        assert!(!run.receipt.clean_exit);
        assert!(run.status.is_none());
    }

    fn run_sh_cancellable(
        cmd: &str,
        limits: &SupervisorLimits,
        cancel: &dyn Fn() -> bool,
    ) -> SupervisedRun {
        run_supervised_cancellable(
            "/bin/sh",
            &["-c", cmd],
            std::env::temp_dir().as_path(),
            limits,
            cancel,
        )
    }

    #[test]
    fn cancel_before_start_never_runs_child_to_completion() {
        let limits = SupervisorLimits {
            timeout: Duration::from_secs(30),
            term_grace: Duration::from_millis(500),
            ..Default::default()
        };
        // Cancel is already true, so the first poll stops the child before it
        // can finish; a 30s child guarantees the cancel branch is the one hit.
        let started = Instant::now();
        let run = run_sh_cancellable("sleep 30", &limits, &|| true);
        assert_eq!(run.receipt.termination, Termination::Cancelled);
        assert!(!run.receipt.clean_exit);
        assert!(started.elapsed() < Duration::from_secs(30));
    }

    #[test]
    fn cancel_polled_during_execution_terminates_in_flight_child() {
        let limits = SupervisorLimits {
            timeout: Duration::from_secs(30),
            term_grace: Duration::from_millis(500),
            ..Default::default()
        };
        let start = Instant::now();
        let cancel_at = start + Duration::from_millis(120);
        let should_cancel = move || Instant::now() >= cancel_at;
        let started = Instant::now();
        let run = run_sh_cancellable("sleep 30", &limits, &should_cancel);
        assert_eq!(run.receipt.termination, Termination::Cancelled);
        assert!(!run.receipt.clean_exit);
        assert!(started.elapsed() >= Duration::from_millis(100));
        assert!(started.elapsed() < Duration::from_secs(30));
    }

    #[test]
    fn cancel_false_completion_reports_clean_exit() {
        let run = run_sh_cancellable("echo ran-to-completion", &default_limits(), &|| false);
        assert_eq!(run.receipt.termination, Termination::Exited(0));
        assert!(run.receipt.clean_exit);
        assert!(run.receipt.output.contains("ran-to-completion"));
    }
}
