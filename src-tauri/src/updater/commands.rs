use crate::updater::{UpdaterService, UpdateCheckResult, verify_update_signature};
use std::sync::OnceLock;
use std::time::Duration;

static UPDATER_SERVICE: OnceLock<crate::updater::StubUpdaterService> = OnceLock::new();

fn updater() -> &'static crate::updater::StubUpdaterService {
    UPDATER_SERVICE.get_or_init(|| crate::updater::StubUpdaterService::new(Duration::from_secs(10)))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct CheckUpdatesRequest {
    pub endpoint: String,
    pub current_version: String,
}

#[tauri::command]
pub fn check_updates(request: CheckUpdatesRequest) -> Result<UpdateCheckResult, String> {
    Ok(updater().check(&request.endpoint, &request.current_version))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VerifySignatureRequest {
    pub artifact_path: String,
    pub signature_path: String,
    pub public_key_path: String,
}

#[tauri::command]
pub fn verify_signature(request: VerifySignatureRequest) -> Result<bool, String> {
    verify_update_signature(&request.artifact_path, &request.signature_path, &request.public_key_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_updates_returns_no_update_from_stub_service() {
        let result = check_updates(CheckUpdatesRequest {
            endpoint: "https://example.com/updater".into(),
            current_version: "0.1.0".into(),
        });
        assert!(result.is_ok());
        let payload = result.unwrap();
        assert!(!payload.available);
        assert!(payload.manifest.is_none());
    }

    #[test]
    fn verify_signature_returns_error_when_paths_invalid() {
        let result = verify_signature(VerifySignatureRequest {
            artifact_path: "/missing/artifact".into(),
            signature_path: "/missing/sig".into(),
            public_key_path: "/missing/pub".into(),
        });
        assert!(result.is_err());
    }
}
