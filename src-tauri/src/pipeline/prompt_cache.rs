//! Generation Prompt Cache (ported from devcode GenerationPromptCache.ts)
//!
//! TTL-aware prompt template cache and content-hash based generation result cache.
//! Prevents redundant LLM calls across rebuilds. Pure module — no I/O, no DI.

#![allow(dead_code)]  // ROADMAP parity port: kept for planned wiring; prune when the Q-item closes.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

const PROMPT_CACHE_TTL_MS: u128 = 15 * 60 * 1000;
const PROMPT_CACHE_MAX_ENTRIES: usize = 50;

const RESULT_CACHE_TTL_MS: u128 = 30 * 60 * 1000;
const RESULT_CACHE_MAX_ENTRIES: usize = 50;

fn to_radix_36(mut value: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".into();
    }
    let mut result = Vec::new();
    while value > 0 {
        result.push(DIGITS[(value % 36) as usize] as char);
        value /= 36;
    }
    result.reverse();
    result.into_iter().collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSpecification {
    pub title: Option<String>,
    pub description: Option<String>,
    pub tech_stack: Vec<String>,
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlanningContext {
    pub architecture_summary: Option<String>,
    pub security_requirements: Vec<String>,
    pub plan_review_findings: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PromptTemplateCache {
    entries: BTreeMap<String, CachedPromptEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedPromptEntry {
    content: String,
    created_at_ms: u128,
}

impl PromptTemplateCache {
    pub fn new() -> Self { Self::default() }

    fn now_ms() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    }

    fn evict_expired(&mut self) {
        let now = Self::now_ms();
        let expired: Vec<String> = self.entries.iter()
            .filter(|(_, e)| now.saturating_sub(e.created_at_ms) > PROMPT_CACHE_TTL_MS)
            .map(|(k, _)| k.clone())
            .collect();
        for key in expired { self.entries.remove(&key); }
    }

    pub fn build_static_prompt_sections(
        &mut self,
        spec: &ProjectSpecification,
        planning_context: Option<&PlanningContext>,
        mode: Option<&str>,
    ) -> String {
        if self.entries.len() > 0 && self.entries.len() % 10 == 0 {
            self.evict_expired();
        }

        let key = Self::cache_key(spec, planning_context, mode);
        let now = Self::now_ms();
        if let Some(entry) = self.entries.get(&key) {
            if now.saturating_sub(entry.created_at_ms) <= PROMPT_CACHE_TTL_MS {
                return entry.content.clone();
            }
        }

        let mut sections: Vec<String> = Vec::new();

        if !spec.title.is_none() || !spec.description.is_none() || !spec.tech_stack.is_empty() || !spec.features.is_empty() {
            sections.push(String::new());
            if let Some(title) = &spec.title {
                sections.push(format!("Project: {}", title));
            }
            if let Some(desc) = &spec.description {
                sections.push(format!("Description: {}", desc));
            }
            if !spec.tech_stack.is_empty() {
                sections.push(format!("Tech stack: {}", spec.tech_stack.join(", ")));
            }
            if !spec.features.is_empty() {
                sections.push(format!("Features: {}", spec.features.join(", ")));
            }
        }

        if let Some(ctx) = planning_context {
            if let Some(summary) = &ctx.architecture_summary {
                sections.push(String::new());
                sections.push("Architecture Constraints:".into());
                sections.push(summary.chars().take(1000).collect());
            }
            if !ctx.security_requirements.is_empty() {
                sections.push(String::new());
                sections.push("Security Requirements:".into());
                for req in &ctx.security_requirements {
                    sections.push(format!("- {}", req));
                }
            }
            if !ctx.plan_review_findings.is_empty() {
                sections.push(String::new());
                sections.push("Review Findings (apply during code generation):".into());
                let (shown, overflow) = if ctx.plan_review_findings.len() > 25 {
                    (&ctx.plan_review_findings[..25], ctx.plan_review_findings.len() - 25)
                } else {
                    (ctx.plan_review_findings.as_slice(), 0)
                };
                for finding in shown {
                    sections.push(format!("- {}", finding));
                }
                if overflow > 0 {
                    sections.push(format!("  (...and {} more finding(s) omitted for prompt budget)", overflow));
                }
            }
        }

        let result = sections.join("\n");
        self.entries.insert(key, CachedPromptEntry { content: result.clone(), created_at_ms: now });

        if self.entries.len() > PROMPT_CACHE_MAX_ENTRIES {
            let oldest = self.entries.keys().next().cloned();
            if let Some(k) = oldest { self.entries.remove(&k); }
        }

        result
    }

    fn cache_key(spec: &ProjectSpecification, planning_context: Option<&PlanningContext>, mode: Option<&str>) -> String {
        let parts = [
            spec.title.clone().unwrap_or_default(),
            spec.description.clone().unwrap_or_default(),
            spec.tech_stack.join(","),
            spec.features.len().to_string(),
            planning_context.and_then(|c| c.architecture_summary.as_ref()).map(|s| s.chars().take(200).collect::<String>()).unwrap_or_default(),
            planning_context.map(|c| c.plan_review_findings.len().to_string()).unwrap_or_default(),
            mode.unwrap_or("create").to_string(),
        ];
        parts.join("|")
    }
}

#[derive(Debug, Clone, Default)]
pub struct GenerationResultCache {
    entries: BTreeMap<String, CachedResultEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedResultEntry {
    content: String,
    created_at_ms: u128,
}

impl GenerationResultCache {
    pub fn new() -> Self { Self::default() }

    fn now_ms() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    }

    /// Fast 32-bit djb2-style hash for cache keys.
    pub fn hash_content(content: &str) -> String {
        let mut hash: u64 = 5381;
        for ch in content.chars() {
            hash = ((hash << 5).wrapping_add(hash)).wrapping_add(ch as u64) & 0xffffffff;
        }
        to_radix_36(hash)
    }

    pub fn cache_key(file_path: &str, language: &str, specification: &str, symbol_contract_hash: Option<&str>) -> String {
        [
            file_path,
            language,
            &Self::hash_content(specification),
            symbol_contract_hash.unwrap_or(""),
        ].join(":")
    }

    pub fn get(&mut self, key: &str) -> Option<String> {
        let entry = self.entries.get(key)?;
        if Self::now_ms().saturating_sub(entry.created_at_ms) > RESULT_CACHE_TTL_MS {
            self.entries.remove(key);
            return None;
        }
        Some(entry.content.clone())
    }

    pub fn set(&mut self, key: &str, content: &str) {
        if self.entries.len() > 0 && self.entries.len() % 10 == 0 {
            let now = Self::now_ms();
            let expired: Vec<String> = self.entries.iter()
                .filter(|(_, e)| now.saturating_sub(e.created_at_ms) > RESULT_CACHE_TTL_MS)
                .map(|(k, _)| k.clone())
                .collect();
            for k in expired { self.entries.remove(&k); }
        }

        let now = Self::now_ms();
        self.entries.insert(key.into(), CachedResultEntry { content: content.into(), created_at_ms: now });

        if self.entries.len() > RESULT_CACHE_MAX_ENTRIES {
            let oldest = self.entries.keys().next().cloned();
            if let Some(k) = oldest { self.entries.remove(&k); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_cache_returns_same_content() {
        let mut cache = PromptTemplateCache::new();
        let spec = ProjectSpecification {
            title: Some("My App".into()),
            description: Some("A test app".into()),
            tech_stack: vec!["React".into(), "TypeScript".into()],
            features: vec!["auth".into()],
        };
        let ctx = PlanningContext {
            architecture_summary: Some("MVC".into()),
            security_requirements: vec!["HTTPS".into()],
            plan_review_findings: vec!["add tests".into()],
        };
        let first = cache.build_static_prompt_sections(&spec, Some(&ctx), Some("create"));
        let second = cache.build_static_prompt_sections(&spec, Some(&ctx), Some("create"));
        assert_eq!(first, second);
        assert!(first.contains("MVC"));
        assert!(first.contains("add tests"));
    }

    #[test]
    fn test_findings_truncated_at_25() {
        let mut cache = PromptTemplateCache::new();
        let spec = ProjectSpecification { title: None, description: None, tech_stack: vec![], features: vec![] };
        let ctx = PlanningContext {
            architecture_summary: None,
            security_requirements: vec![],
            plan_review_findings: (0..30).map(|i| format!("finding {}", i)).collect(),
        };
        let prompt = cache.build_static_prompt_sections(&spec, Some(&ctx), None);
        assert!(prompt.contains("finding 0"));
        assert!(!prompt.contains("finding 25"));
        assert!(prompt.contains("5 more finding(s) omitted"));
    }

    #[test]
    fn test_result_cache_round_trip() {
        let mut cache = GenerationResultCache::new();
        let key = GenerationResultCache::cache_key("src/App.tsx", "typescript", "spec", Some("abc"));
        assert!(cache.get(&key).is_none());
        cache.set(&key, "export default App;");
        assert_eq!(cache.get(&key), Some("export default App;".into()));
    }

    #[test]
    fn test_result_cache_key_determinism() {
        let k1 = GenerationResultCache::cache_key("src/App.tsx", "typescript", "spec", None);
        let k2 = GenerationResultCache::cache_key("src/App.tsx", "typescript", "spec", None);
        assert_eq!(k1, k2);
    }

    #[test]
    fn test_hash_content_is_stable() {
        let h1 = GenerationResultCache::hash_content("hello");
        let h2 = GenerationResultCache::hash_content("hello");
        assert_eq!(h1, h2);
        assert_ne!(h1, GenerationResultCache::hash_content("world"));
    }
}
