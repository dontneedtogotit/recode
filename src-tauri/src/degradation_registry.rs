use std::collections::HashSet;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Hash)]
pub struct DegradationRecord {
    pub subsystem: String,
    pub operation: String,
    pub source: String,
    pub timestamp: i64,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DegradationRegistry {
    active: HashSet<DegradationRecord>,
}

impl DegradationRecord {
    pub fn new(subsystem: impl Into<String>, operation: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            subsystem: subsystem.into(),
            operation: operation.into(),
            source: source.into(),
            timestamp: chrono_now(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

impl DegradationRegistry {
    pub fn new() -> Self {
        Self {
            active: HashSet::new(),
        }
    }

    pub fn register(&mut self, record: DegradationRecord) {
        let key = Self::key(&record);
        let mut updated = false;
        for existing in &self.active {
            if Self::key(existing) == key {
                updated = true;
                break;
            }
        }

        if !updated {
            self.active.insert(record);
        }
    }

    pub fn clear(&mut self, subsystem: &str) {
        self.active.retain(|record| record.subsystem != subsystem);
    }

    pub fn force_clear(&mut self) {
        self.active.clear();
    }

    pub fn get_active(&self) -> Vec<&DegradationRecord> {
        let mut records: Vec<&DegradationRecord> = self.active.iter().collect();
        records.sort_by_key(|record| record.timestamp);
        records
    }

    pub fn has_active(&self) -> bool {
        !self.active.is_empty()
    }

    fn key(record: &DegradationRecord) -> String {
        format!("{}:{}:{}", record.subsystem, record.operation, record.source)
    }
}

impl Default for DegradationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn degradation_registry() -> &'static std::sync::Mutex<DegradationRegistry> {
    use std::sync::OnceLock;
    static REGISTRY: OnceLock<std::sync::Mutex<DegradationRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| std::sync::Mutex::new(DegradationRegistry::new()))
}

fn chrono_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_creates_new_degradation() {
        let mut registry = DegradationRegistry::new();
        let record = DegradationRecord::new("gateway", "resolve", "provider-a");
        registry.register(record.clone());
        assert!(registry.has_active());
        assert_eq!(registry.get_active().len(), 1);
    }

    #[test]
    fn register_updates_existing_degradation() {
        let mut registry = DegradationRegistry::new();
        let first = DegradationRecord::new("gateway", "resolve", "provider-a");
        registry.register(first);
        let second = DegradationRecord::new("gateway", "resolve", "provider-a");
        registry.register(second);
        assert_eq!(registry.get_active().len(), 1);
    }

    #[test]
    fn clear_removes_subsystem_degradations() {
        let mut registry = DegradationRegistry::new();
        registry.register(DegradationRecord::new("gateway", "resolve", "provider-a"));
        registry.register(DegradationRecord::new("gateway", "resolve", "provider-b"));
        registry.register(DegradationRecord::new("diagnostics", "collect", "runtime"));
        registry.clear("gateway");
        assert_eq!(registry.get_active().len(), 1);
        assert!(registry.get_active()[0].subsystem == "diagnostics");
    }

    #[test]
    fn force_clear_empties_registry() {
        let mut registry = DegradationRegistry::new();
        registry.register(DegradationRecord::new("gateway", "resolve", "provider-a"));
        registry.register(DegradationRecord::new("diagnostics", "collect", "runtime"));
        registry.force_clear();
        assert!(!registry.has_active());
    }

    #[test]
    fn get_active_returns_sorted_records() {
        let mut registry = DegradationRegistry::new();
        let mut first = DegradationRecord::new("gateway", "resolve", "provider-a");
        first.timestamp = 1;
        let mut second = DegradationRecord::new("diagnostics", "collect", "runtime");
        second.timestamp = 2;
        registry.register(first);
        registry.register(second);
        let active = registry.get_active();
        assert_eq!(active.len(), 2);
        assert_eq!(active[0].subsystem, "gateway");
        assert_eq!(active[1].subsystem, "diagnostics");
    }
}
