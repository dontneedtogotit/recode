use std::time::Duration;

pub mod commands;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateManifest {
    pub url: String,
    pub version: String,
    pub notes: Option<String>,
    pub sha512: Option<String>,
    pub pub_date: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateCheckResult {
    pub available: bool,
    pub manifest: Option<UpdateManifest>,
    pub signature_valid: Option<bool>,
}

pub trait UpdaterService {
    fn check(&self, endpoint: &str, current_version: &str) -> UpdateCheckResult;
    fn timeout(&self) -> Duration;
}

pub struct StubUpdaterService {
    pub timeout: Duration,
}

impl StubUpdaterService {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl UpdaterService for StubUpdaterService {
    fn check(&self, _endpoint: &str, _current_version: &str) -> UpdateCheckResult {
        UpdateCheckResult {
            available: false,
            manifest: None,
            signature_valid: None,
        }
    }

    fn timeout(&self) -> Duration {
        self.timeout
    }
}

pub fn verify_update_signature(
    artifact_path: &str,
    signature_path: &str,
    public_key_path: &str,
) -> Result<bool, String> {
    if !std::path::Path::new(artifact_path).exists() {
        return Err(format!("artifact not found: {artifact_path}"));
    }
    if !std::path::Path::new(signature_path).exists() {
        return Err(format!("signature not found: {signature_path}"));
    }
    if !std::path::Path::new(public_key_path).exists() {
        return Err(format!("public key not found: {public_key_path}"));
    }

    let output = std::process::Command::new("openssl")
        .args([
            "dgst",
            "-sha256",
            "-verify",
            public_key_path,
            "-signature",
            signature_path,
            artifact_path,
        ])
        .output()
        .map_err(|e| format!("failed to run openssl: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("openssl verification failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim() == "Verified OK")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_updater_reports_no_update_by_default() {
        let service = StubUpdaterService::new(Duration::from_secs(10));
        let result = service.check("https://example.com/updater", "0.1.0");
        assert!(!result.available);
        assert!(result.manifest.is_none());
        assert!(result.signature_valid.is_none());
    }

    #[test]
    fn stub_updater_reports_configured_timeout() {
        let service = StubUpdaterService::new(Duration::from_secs(7));
        assert_eq!(service.timeout(), Duration::from_secs(7));
    }

    #[test]
    fn verify_update_signature_errors_when_artifact_missing() {
        let result = verify_update_signature("/missing/artifact", "/missing/sig", "/missing/pub");
        assert!(result.is_err());
    }

    #[test]
    fn verify_update_signature_errors_when_signature_missing() {
        let result = verify_update_signature("src-tauri/src/main.rs", "/missing/sig", "/missing/pub");
        assert!(result.is_err());
    }

    #[test]
    fn verify_update_signature_errors_when_public_key_missing() {
        let result = verify_update_signature("src-tauri/src/main.rs", "src-tauri/src/main.rs", "/missing/pub");
        assert!(result.is_err());
    }
}
