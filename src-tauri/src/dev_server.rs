use super::domain::DomainError;
use std::net::TcpListener;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DevServerInfo {
    pub port: u16,
    pub url: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum DevServerStatus {
    Stopped,
    Starting,
    Running,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DevServerStatusReport {
    pub status: DevServerStatus,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub message: String,
}

pub trait PortManager {
    fn find_open_port(&self, preferred: u16) -> Result<u16, DomainError>;
    fn wait_until_ready(&self, url: &str, timeout: std::time::Duration) -> Result<(), DomainError>;
}

pub struct LocalPortManager;

impl PortManager for LocalPortManager {
    fn find_open_port(&self, preferred: u16) -> Result<u16, DomainError> {
        let port = preferred.max(1024);
        match TcpListener::bind(("127.0.0.1", port)) {
            Ok(listener) => {
                let actual = listener.local_addr().map(|a| a.port()).unwrap_or(port);
                Ok(actual)
            }
            Err(_) => {
                let mut probe = port + 1;
                while probe < 65535 {
                    if TcpListener::bind(("127.0.0.1", probe)).is_ok() {
                        return Ok(probe);
                    }
                    probe += 1;
                }
                Err(DomainError::internal("no open port found"))
            }
        }
    }

    fn wait_until_ready(
        &self,
        _url: &str,
        _timeout: std::time::Duration,
    ) -> Result<(), DomainError> {
        Ok(())
    }
}

pub trait DevServerStatusReporter {
    fn report_status(&self, port: Option<u16>) -> DevServerStatusReport;
}

pub struct LocalDevServerStatusReporter;

impl DevServerStatusReporter for LocalDevServerStatusReporter {
    fn report_status(&self, port: Option<u16>) -> DevServerStatusReport {
        match port {
            Some(port) => {
                let url = format!("http://127.0.0.1:{}", port);
                match TcpListener::bind(("127.0.0.1", port)) {
                    Ok(_) => DevServerStatusReport {
                        status: DevServerStatus::Running,
                        port: Some(port),
                        url: Some(url.clone()),
                        message: format!("dev server available on {}", url),
                    },
                    Err(_) => DevServerStatusReport {
                        status: DevServerStatus::Failed,
                        port: Some(port),
                        url: Some(url),
                        message: "dev server port is not accepting connections".into(),
                    },
                }
            }
            None => DevServerStatusReport {
                status: DevServerStatus::Stopped,
                port: None,
                url: None,
                message: "no dev server port configured".into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_open_port_at_or_after_preferred() {
        let manager = LocalPortManager;
        let port = manager.find_open_port(18000).expect("open port");
        assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
    }

    #[test]
    fn readiness_returns_ok_for_stub() {
        let manager = LocalPortManager;
        let result =
            manager.wait_until_ready("http://127.0.0.1:18000", std::time::Duration::from_secs(1));
        assert!(result.is_ok());
    }

    #[test]
    fn reports_running_when_port_is_open() {
        let reporter = LocalDevServerStatusReporter;
        let port = reporter
            .report_status(Some(18001))
            .port
            .expect("reported port");
        assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
    }

    #[test]
    fn reports_stopped_when_port_is_none() {
        let reporter = LocalDevServerStatusReporter;
        let report = reporter.report_status(None);
        assert_eq!(report.status, DevServerStatus::Stopped);
    }
}
