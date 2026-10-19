//! Supervised verification stages (ROADMAP R1-2, milestone M2).
//!
//! Real subprocess receipts for the typecheck, bundle, tests and startup
//! gates, built on the consented process supervisor
//! ([`super::supervisor::run_supervised`]). Only [`ExecutionConsent::Granted`]
//! ever spawns a child; `NotSupplied` and `Denied` report the gate `Unknown`,
//! never a pass, so an unconsented run cannot be mistaken for a verified
//! build. A missing runner (npm/node unavailable) is likewise `Unknown`,
//! never an executed failure.
//!
//! Allowlist: the script stages run exactly `npm run type-check`,
//! `npm run build` and `npm run test:run -- --passWithNoTests` in the
//! workspace. The startup stage runs a fixed node boot-check script that
//! starts `npm run dev -- --port <ephemeral> --strictPort --host 127.0.0.1`,
//! polls `127.0.0.1` until the server responds, then kills the whole server
//! process group and exits 0/1 by itself — so the supervisor observes a real
//! self-terminating command instead of a killed server. Nothing
//! user-controllable is ever interpolated into argv.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use super::supervisor::{
    run_supervised, run_supervised_cancellable, ExecutionConsent, SupervisorLimits,
    SupervisorReceipt, Termination,
};
use super::types::GeneratedFile;
use super::verify::{
    ExecutionEvidence, GateEvidence, GateStatus, VerificationResult, VerificationStage,
};

/// Bounds for `<runner> --version` probes: tiny commands that must answer
/// immediately or be treated as missing.
fn version_limits() -> SupervisorLimits {
    SupervisorLimits {
        timeout: Duration::from_secs(15),
        term_grace: Duration::from_secs(1),
        max_output_bytes: 8 * 1024,
    }
}

/// Bounds for the consented `npm install` prerequisite step.
pub fn install_limits() -> SupervisorLimits {
    SupervisorLimits {
        timeout: Duration::from_secs(300),
        term_grace: Duration::from_secs(5),
        max_output_bytes: 256 * 1024,
    }
}

/// Bounds for the supervised script gates.
fn script_limits(timeout_secs: u64) -> SupervisorLimits {
    SupervisorLimits {
        timeout: Duration::from_secs(timeout_secs),
        term_grace: Duration::from_secs(5),
        max_output_bytes: 256 * 1024,
    }
}

/// Probe a runner's version under supervision. `None` means the runner is
/// missing or broken; callers must treat that as unknown, never as a pass
/// or a measured failure.
pub fn supervised_tool_version(program: &str, cwd: &Path) -> Option<String> {
    let run = run_supervised(program, &["--version"], cwd, &version_limits());
    match run.receipt.termination {
        Termination::Exited(0) => run
            .receipt
            .output
            .lines()
            .next()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

/// Consented prerequisite for the script gates: a bounded, supervised
/// `npm install` when the workspace declares a package.json but has no
/// `node_modules` yet (F07 network/script consent). Returns a short note for
/// the verification summary, or "" when nothing ran.
pub fn install_step(workspace_root: &Path, consent: ExecutionConsent) -> String {
    install_step_cancellable(workspace_root, consent, &|| false)
}

/// Cancellable install: `should_cancel` is polled while npm install runs; a
/// `true` stops it with the bounded TERM-then-KILL escalation.
pub fn install_step_cancellable(
    workspace_root: &Path,
    consent: ExecutionConsent,
    should_cancel: &dyn Fn() -> bool,
) -> String {
    if !consent.is_granted() || !workspace_root.join("package.json").is_file() {
        return String::new();
    }
    if workspace_root.join("node_modules").is_dir() {
        return String::new();
    }
    let run = run_supervised_cancellable(
        "npm",
        &["install", "--no-audit", "--no-fund"],
        workspace_root,
        &install_limits(),
        should_cancel,
    );
    match &run.receipt.termination {
        Termination::Exited(0) => {
            format!("install: npm install completed in {}ms", run.receipt.duration_ms)
        }
        Termination::Exited(code) => format!(
            "install: npm install exited with {code}: {}",
            tail(&run.receipt.output, 300)
        ),
        Termination::TimedOut => format!(
            "install: npm install timed out after {}ms",
            run.receipt.duration_ms
        ),
        Termination::Cancelled => format!(
            "install: npm install cancelled after {}ms",
            run.receipt.duration_ms
        ),
        Termination::FailedToStart(error) => {
            format!("install: npm install failed to start: {error}")
        }
    }
}

/// Last `max_chars` characters of captured output, for gate details.
fn tail(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let count = trimmed.chars().count();
    if count <= max_chars {
        return trimmed.to_string();
    }
    trimmed.chars().skip(count - max_chars).collect()
}

/// The gate is applicable but was not executed: no scoped consent.
fn unconsented(gate: &str) -> VerificationResult {
    VerificationResult::unknown(
        gate,
        "Not executed: no scoped execution consent for this run (required to run supervised npm commands).",
        Some(true),
    )
}

/// The gate is applicable but its runner is missing: unknown, never an
/// executed failure.
fn runner_missing(gate: &str, tool: &str) -> VerificationResult {
    let detail = format!("Not executed: {tool} runner unavailable; install it and rerun verification.");
    VerificationResult::unknown(gate, &detail, Some(true))
}

/// Turn a supervised run into a gate result with a real execution receipt.
/// `Exited(0)` passes; any other termination fails with the receipt attached.
fn receipt_result(
    gate: &str,
    receipt: &SupervisorReceipt,
    tool_version: String,
) -> VerificationResult {
    let command_line = receipt.command.join(" ");
    let (exit_code, status, detail) = match &receipt.termination {
        Termination::Exited(0) => (
            0,
            GateStatus::Passed,
            format!("`{command_line}` passed in {}ms", receipt.duration_ms),
        ),
        Termination::Exited(code) => (
            *code,
            GateStatus::Failed,
            format!(
                "`{command_line}` exited with {code}: {}",
                tail(&receipt.output, 400)
            ),
        ),
        Termination::TimedOut => (
            124,
            GateStatus::Failed,
            format!(
                "`{command_line}` timed out after {}ms: {}",
                receipt.duration_ms,
                tail(&receipt.output, 400)
            ),
        ),
        Termination::Cancelled => (
            130,
            GateStatus::Failed,
            format!(
                "`{command_line}` cancelled after {}ms: {}",
                receipt.duration_ms,
                tail(&receipt.output, 400)
            ),
        ),
        Termination::FailedToStart(error) => (
            127,
            GateStatus::Failed,
            format!("`{command_line}` failed to start: {error}"),
        ),
    };
    let passed = status == GateStatus::Passed;
    VerificationResult {
        passed,
        degraded: false,
        gate: gate.to_string(),
        detail,
        evidence: Some(GateEvidence {
            schema_version: 1,
            status,
            required: true,
            applicable: Some(true),
            execution: Some(ExecutionEvidence {
                command: receipt.command.clone(),
                exit_code,
                duration_ms: receipt.duration_ms,
                tool_version,
            }),
        }),
    }
}

/// One supervised `npm run <script>` gate with fixed extra arguments.
pub struct SupervisedScriptStage {
    gate: &'static str,
    script: &'static str,
    extra_args: &'static [&'static str],
    consent: ExecutionConsent,
    tool_version: Option<String>,
    limits: SupervisorLimits,
}

impl SupervisedScriptStage {
    fn new(
        gate: &'static str,
        script: &'static str,
        extra_args: &'static [&'static str],
        consent: ExecutionConsent,
        tool_version: Option<String>,
        limits: SupervisorLimits,
    ) -> Self {
        Self { gate, script, extra_args, consent, tool_version, limits }
    }

    /// `npm run type-check` — the typecheck gate (tsc --noEmit).
    pub fn typecheck(consent: ExecutionConsent, tool_version: Option<String>) -> Self {
        Self::new("typecheck", "type-check", &[], consent, tool_version, script_limits(180))
    }

    /// `npm run build` — the bundle gate (tsc -b && vite build).
    pub fn bundle(consent: ExecutionConsent, tool_version: Option<String>) -> Self {
        Self::new("bundle", "build", &[], consent, tool_version, script_limits(300))
    }

    /// `npm run test:run -- --passWithNoTests` — the tests gate
    /// (`vitest run`, never watch mode). The scaffolded project may
    /// legitimately contain no test files; `--passWithNoTests` keeps that an
    /// honest pass with visible output, while real test failures still exit
    /// non-zero.
    pub fn tests(consent: ExecutionConsent, tool_version: Option<String>) -> Self {
        Self::new(
            "tests",
            "test:run",
            &["--", "--passWithNoTests"],
            consent,
            tool_version,
            script_limits(300),
        )
    }
}

impl VerificationStage for SupervisedScriptStage {
    fn name(&self) -> &'static str {
        self.gate
    }

    fn verify(&self, workspace_root: &Path, _files: &[GeneratedFile]) -> VerificationResult {
        if !self.consent.is_granted() {
            return unconsented(self.gate);
        }
        let Some(tool_version) = self.tool_version.clone() else {
            return runner_missing(self.gate, "npm");
        };
        let mut argv: Vec<&str> = vec!["run", self.script];
        argv.extend_from_slice(self.extra_args);
        let run = run_supervised("npm", &argv, workspace_root, &self.limits);
        receipt_result(self.gate, &run.receipt, tool_version)
    }
}

/// Fixed node boot-check script for the startup gate. Starts `npm run dev`
/// detached in its own process group on an ephemeral port, polls
/// `127.0.0.1` until the server serves `/`, then kills the whole server
/// group (TERM, then KILL) and exits 0; exits 1 when the server dies first
/// or never becomes ready within the deadline.
const BOOT_CHECK_SCRIPT: &str = r#"'use strict';
const { spawn } = require('child_process');
const http = require('http');
const net = require('net');
const READY_TIMEOUT_MS = 45000;
function sleep(ms) { return new Promise((resolve) => setTimeout(resolve, ms)); }
function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.on('error', reject);
    server.listen(0, '127.0.0.1', () => {
      const port = server.address().port;
      server.close(() => resolve(port));
    });
  });
}
function serves(port) {
  return new Promise((resolve) => {
    const request = http.get({ host: '127.0.0.1', port, path: '/', timeout: 2000 }, (response) => {
      const ok = response.statusCode >= 200 && response.statusCode < 500;
      response.resume();
      resolve(ok);
    });
    request.on('timeout', () => { request.destroy(); resolve(false); });
    request.on('error', () => resolve(false));
  });
}
(async () => {
  const port = await freePort();
  const child = spawn('npm', ['run', 'dev', '--', '--port', String(port), '--strictPort', '--host', '127.0.0.1'], {
    cwd: process.cwd(),
    detached: true,
    stdio: 'ignore',
    env: process.env,
  });
  child.on('error', (error) => {
    console.error('failed to start dev server: ' + error.message);
    process.exit(1);
  });
  const killGroup = (signal) => {
    try { process.kill(-child.pid, signal); } catch (_) {}
  };
  const deadline = Date.now() + READY_TIMEOUT_MS;
  while (Date.now() < deadline) {
    if (child.exitCode !== null || child.signalCode !== null) {
      killGroup('SIGKILL');
      console.error('dev server exited before readiness');
      process.exit(1);
    }
    if (await serves(port)) {
      killGroup('SIGTERM');
      await sleep(1500);
      killGroup('SIGKILL');
      console.log('boot check: dev server became ready on port ' + port);
      process.exit(0);
    }
    await sleep(300);
  }
  killGroup('SIGTERM');
  await sleep(1500);
  killGroup('SIGKILL');
  console.error('dev server did not become ready within ' + READY_TIMEOUT_MS + 'ms');
  process.exit(1);
})().catch((error) => {
  console.error('boot check failed: ' + error.message);
  process.exit(1);
});
"#;

static BOOT_SCRIPT_SEQ: AtomicU64 = AtomicU64::new(0);

/// Startup gate: boot check proving the generated app actually starts.
/// Runs the fixed node script above under the supervisor so the receipt is a
/// real self-terminating command (exit 0 == reached readiness), not a
/// killed server.
pub struct StartupBootStage {
    consent: ExecutionConsent,
    tool_version: Option<String>,
    limits: SupervisorLimits,
}

impl StartupBootStage {
    pub fn new(consent: ExecutionConsent, tool_version: Option<String>) -> Self {
        Self { consent, tool_version, limits: script_limits(90) }
    }

    fn write_boot_script() -> std::io::Result<PathBuf> {
        let seq = BOOT_SCRIPT_SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "recode-boot-check-{}-{}-{seq}",
            std::process::id(),
            crate::event_bus::now_millis()
        ));
        std::fs::create_dir_all(&dir)?;
        let script = dir.join("boot-check.js");
        std::fs::write(&script, BOOT_CHECK_SCRIPT)?;
        Ok(script)
    }
}

impl VerificationStage for StartupBootStage {
    fn name(&self) -> &'static str {
        "startup"
    }

    fn verify(&self, workspace_root: &Path, _files: &[GeneratedFile]) -> VerificationResult {
        if !self.consent.is_granted() {
            return unconsented("startup");
        }
        let Some(tool_version) = self.tool_version.clone() else {
            return runner_missing("startup", "node");
        };
        let script = match Self::write_boot_script() {
            Ok(path) => path,
            Err(error) => {
                let detail = format!("Not executed: could not stage the boot-check script: {error}");
                return VerificationResult::unknown("startup", &detail, Some(true));
            }
        };
        let script_arg = script.to_string_lossy().into_owned();
        let run = run_supervised("node", &[&script_arg], workspace_root, &self.limits);
        // Best-effort cleanup of the staged script; the receipt keeps the
        // exact argv that ran.
        if let Some(parent) = script.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
        receipt_result("startup", &run.receipt, tool_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn withheld_or_denied_consent_reports_unknown_without_spawning() {
        let root = tempfile::tempdir().unwrap();
        for consent in [ExecutionConsent::NotSupplied, ExecutionConsent::Denied] {
            let checks = [
                SupervisedScriptStage::typecheck(consent, Some("npm 10.0.0".into()))
                    .verify(root.path(), &[]),
                SupervisedScriptStage::bundle(consent, Some("npm 10.0.0".into()))
                    .verify(root.path(), &[]),
                SupervisedScriptStage::tests(consent, Some("npm 10.0.0".into()))
                    .verify(root.path(), &[]),
                StartupBootStage::new(consent, Some("v22.0.0".into()))
                    .verify(root.path(), &[]),
            ];
            for result in checks {
                assert!(!result.passed, "gate {} must not pass without consent", result.gate);
                let evidence = result.evidence.expect("evidence required");
                assert_eq!(evidence.status, GateStatus::Unknown);
                assert_eq!(evidence.applicable, Some(true));
                assert!(evidence.execution.is_none(), "nothing may execute without consent");
            }
        }
    }

    #[test]
    fn missing_runner_is_unknown_not_a_measured_failure() {
        let root = tempfile::tempdir().unwrap();
        let result =
            SupervisedScriptStage::typecheck(ExecutionConsent::Granted, None)
                .verify(root.path(), &[]);
        assert!(!result.passed);
        let evidence = result.evidence.expect("evidence required");
        assert_eq!(evidence.status, GateStatus::Unknown);
        assert!(evidence.execution.is_none());

        let startup = StartupBootStage::new(ExecutionConsent::Granted, None)
            .verify(root.path(), &[]);
        assert!(startup.evidence.expect("evidence required").execution.is_none());
    }

    #[test]
    fn unconsented_install_step_is_a_no_op() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("package.json"), "{}").unwrap();
        assert!(install_step(root.path(), ExecutionConsent::NotSupplied).is_empty());
        assert!(install_step(root.path(), ExecutionConsent::Denied).is_empty());
    }

    #[test]
    fn existing_node_modules_skips_install() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("package.json"), "{}").unwrap();
        std::fs::create_dir(root.path().join("node_modules")).unwrap();
        // Granted consent would install; the existing node_modules must
        // short-circuit before any spawn.
        assert!(install_step(root.path(), ExecutionConsent::Granted).is_empty());
    }

    #[test]
    fn tail_keeps_only_the_end() {
        assert_eq!(tail("abcdef", 3), "def");
        assert_eq!(tail("abc", 5), "abc");
        assert_eq!(tail("", 3), "");
    }
}
