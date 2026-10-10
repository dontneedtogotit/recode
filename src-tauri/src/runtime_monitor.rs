use std::time::Duration;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MonitorEvent {
    pub ts: String,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentTaskMetrics {
    pub task_id: String,
    pub agent_id: String,
    pub duration_ms: u64,
    pub success: bool,
}

pub trait RuntimeMonitorService {
    fn poll_once(&self) -> Vec<MonitorEvent>;
    fn interval(&self) -> Duration;
    fn record_agent_task(&self, metrics: AgentTaskMetrics);
    fn agent_task_metrics(&self) -> Vec<AgentTaskMetrics>;
    fn record_event(&self, event: MonitorEvent);
    fn recent_events(&self) -> Vec<MonitorEvent>;
    fn clear_events(&self);
}

#[derive(Debug)]
pub struct DefaultRuntimeMonitorService {
    interval: Duration,
    metrics: std::sync::Mutex<Vec<AgentTaskMetrics>>,
    events: std::sync::Mutex<Vec<MonitorEvent>>,
}

static RUNTIME_MONITOR: std::sync::OnceLock<DefaultRuntimeMonitorService> = std::sync::OnceLock::new();

pub fn service() -> &'static DefaultRuntimeMonitorService {
    RUNTIME_MONITOR.get_or_init(|| DefaultRuntimeMonitorService::new(std::time::Duration::from_secs(5)))
}

impl DefaultRuntimeMonitorService {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            metrics: std::sync::Mutex::new(Vec::new()),
            events: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl RuntimeMonitorService for DefaultRuntimeMonitorService {
    fn poll_once(&self) -> Vec<MonitorEvent> {
        let mut guard = self.events.lock().expect("lock events");
        guard.drain(..).collect()
    }

    fn interval(&self) -> Duration {
        self.interval
    }

    fn record_agent_task(&self, metrics: AgentTaskMetrics) {
        let mut guard = self.metrics.lock().expect("lock metrics");
        guard.push(metrics);
    }

    fn agent_task_metrics(&self) -> Vec<AgentTaskMetrics> {
        let guard = self.metrics.lock().expect("lock metrics");
        guard.clone()
    }

    fn record_event(&self, event: MonitorEvent) {
        let mut guard = self.events.lock().expect("lock events");
        guard.push(event);
    }

    fn recent_events(&self) -> Vec<MonitorEvent> {
        let guard = self.events.lock().expect("lock events");
        guard.clone()
    }

    fn clear_events(&self) {
        let mut guard = self.events.lock().expect("lock events");
        guard.clear();
    }
}

#[cfg(test)]
pub(crate) mod tests_helpers {
    use super::*;

    pub fn service() -> &'static DefaultRuntimeMonitorService {
        super::RUNTIME_MONITOR
            .get_or_init(|| DefaultRuntimeMonitorService::new(std::time::Duration::from_secs(5)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_poll_returns_empty_on_healthy_runtime() {
        let service = DefaultRuntimeMonitorService::new(Duration::from_secs(2));
        let events = service.poll_once();
        assert!(events.is_empty());
    }

    #[test]
    fn monitor_reports_configured_interval() {
        let service = DefaultRuntimeMonitorService::new(Duration::from_secs(5));
        assert_eq!(service.interval(), Duration::from_secs(5));
    }

    #[test]
    fn records_and_returns_agent_task_metrics() {
        let service = DefaultRuntimeMonitorService::new(Duration::from_secs(5));
        service.record_agent_task(AgentTaskMetrics {
            task_id: "task-1".into(),
            agent_id: "planner".into(),
            duration_ms: 12,
            success: true,
        });
        let metrics = service.agent_task_metrics();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].task_id, "task-1");
        assert_eq!(metrics[0].duration_ms, 12);
    }

    #[test]
    fn retains_recent_events() {
        let service = DefaultRuntimeMonitorService::new(Duration::from_secs(5));
        service.record_event(MonitorEvent {
            ts: "2026-01-01T00:00:00Z".into(),
            severity: "error".into(),
            message: "timeout".into(),
        });
        service.record_event(MonitorEvent {
            ts: "2026-01-01T00:00:01Z".into(),
            severity: "warn".into(),
            message: "retry".into(),
        });
        let events = service.recent_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].message, "timeout");
    }

    #[test]
    fn poll_once_drains_events() {
        let service = DefaultRuntimeMonitorService::new(Duration::from_secs(5));
        service.record_event(MonitorEvent {
            ts: "2026-01-01T00:00:00Z".into(),
            severity: "error".into(),
            message: "timeout".into(),
        });
        let first = service.poll_once();
        assert_eq!(first.len(), 1);
        assert!(service.recent_events().is_empty());
    }

    #[test]
    fn clear_events_empties_recent_events() {
        let service = DefaultRuntimeMonitorService::new(Duration::from_secs(5));
        service.record_event(MonitorEvent {
            ts: "2026-01-01T00:00:00Z".into(),
            severity: "error".into(),
            message: "timeout".into(),
        });
        service.clear_events();
        assert!(service.recent_events().is_empty());
    }
}
