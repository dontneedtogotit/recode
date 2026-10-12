//! Static + dynamic model catalog.
//!
//! Holds rich per-model metadata (context window, modality, strengths, task
//! affinity, cost tier, family) that the model router uses to score candidates
//! for a task. The catalog is built-in for well-known models and can be
//! extended at runtime with discovered models (see `model_discovery`).
//!
//! The whole backend is synchronous/blocking, so this catalog uses
//! `std::sync::RwLock` and plain (non-async) methods.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{RwLock, RwLockReadGuard};

/// Modalities a model accepts in its prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    Text,
    Image,
    Audio,
    Video,
}

impl Modality {
    pub fn as_str(&self) -> &'static str {
        match self {
            Modality::Text => "text",
            Modality::Image => "image",
            Modality::Audio => "audio",
            Modality::Video => "video",
        }
    }
}

/// Relative cost tier. Routers prefer cheaper tiers for routine tasks and
/// reserve premium tiers for high-stakes work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CostTier {
    Free,
    Low,
    Medium,
    High,
    Premium,
}

impl CostTier {
    /// Numeric weight used by the router (higher = more expensive).
    pub fn weight(&self) -> f32 {
        match self {
            CostTier::Free => 0.0,
            CostTier::Low => 1.0,
            CostTier::Medium => 3.0,
            CostTier::High => 6.0,
            CostTier::Premium => 10.0,
        }
    }
}

/// The task kinds the orchestrator can ask the router to route.
///
/// Compared by `as_str` so the orchestrator can extend it without a breaking
/// type change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    Planning,
    Coding,
    Refactoring,
    Testing,
    Review,
    Debugging,
    Documentation,
    Architecture,
    Reasoning,
    Summarization,
    Translation,
    Extraction,
    Generation,
    Embedding,
    Classification,
    Other,
}

impl TaskKind {
    pub const ALL: [TaskKind; 16] = [
        TaskKind::Planning,
        TaskKind::Coding,
        TaskKind::Refactoring,
        TaskKind::Testing,
        TaskKind::Review,
        TaskKind::Debugging,
        TaskKind::Documentation,
        TaskKind::Architecture,
        TaskKind::Reasoning,
        TaskKind::Summarization,
        TaskKind::Translation,
        TaskKind::Extraction,
        TaskKind::Generation,
        TaskKind::Embedding,
        TaskKind::Classification,
        TaskKind::Other,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            TaskKind::Planning => "planning",
            TaskKind::Coding => "coding",
            TaskKind::Refactoring => "refactoring",
            TaskKind::Testing => "testing",
            TaskKind::Review => "review",
            TaskKind::Debugging => "debugging",
            TaskKind::Documentation => "documentation",
            TaskKind::Architecture => "architecture",
            TaskKind::Reasoning => "reasoning",
            TaskKind::Summarization => "summarization",
            TaskKind::Translation => "translation",
            TaskKind::Extraction => "extraction",
            TaskKind::Generation => "generation",
            TaskKind::Embedding => "embedding",
            TaskKind::Classification => "classification",
            TaskKind::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Option<TaskKind> {
        let v = value.trim().to_ascii_lowercase().replace([' ', '-'], "_");
        Some(match v.as_str() {
            "planning" | "plan" => TaskKind::Planning,
            "coding" | "code" | "implementation" | "implement" => TaskKind::Coding,
            "refactoring" | "refactor" => TaskKind::Refactoring,
            "testing" | "test" | "qa" => TaskKind::Testing,
            "review" | "code_review" => TaskKind::Review,
            "debugging" | "debug" | "diagnosis" | "diagnose" => TaskKind::Debugging,
            "documentation" | "docs" | "doc" => TaskKind::Documentation,
            "architecture" | "arch" | "design" => TaskKind::Architecture,
            "reasoning" | "reason" | "analysis" | "analyze" => TaskKind::Reasoning,
            "summarization" | "summarize" | "summary" => TaskKind::Summarization,
            "translation" | "translate" => TaskKind::Translation,
            "extraction" | "extract" | "structured_extraction" => TaskKind::Extraction,
            "generation" | "generate" | "creative" => TaskKind::Generation,
            "embedding" | "embed" | "vector" => TaskKind::Embedding,
            "classification" | "classify" | "categorize" => TaskKind::Classification,
            "other" | "misc" => TaskKind::Other,
            _ => return None,
        })
    }
}

/// A single model descriptor — everything the router and the UI need to know
/// about one model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    /// Stable unique id: `{provider}/{model_id}`.
    pub id: String,
    /// Provider slug (e.g. `openai`, `anthropic`, `openrouter`, `omniroute`).
    pub provider: String,
    /// Upstream model identifier (e.g. `gpt-4o`, `claude-sonnet-4-5`).
    pub model_id: String,
    /// Human friendly display name.
    pub name: String,
    /// Vendor family (e.g. `gpt`, `claude`, `gemini`, `llama`, `qwen`, `mistral`).
    pub family: String,
    /// Maximum context window in tokens.
    pub context_window: u32,
    /// Maximum tokens the model can emit in a single completion.
    pub max_output_tokens: u32,
    /// Input modalities accepted.
    pub input_modalities: Vec<Modality>,
    /// Whether the model supports tool/function calling.
    pub supports_tools: bool,
    /// Whether the model can emit structured (JSON-schema) output reliably.
    pub supports_structured_output: bool,
    /// Whether the model supports a reasoning/thinking budget.
    pub supports_reasoning: bool,
    /// Cost tier.
    pub cost_tier: CostTier,
    /// One-line human description.
    pub description: String,
    /// What the model is particularly good at (free text, shown in the UI).
    pub strengths: Vec<String>,
    /// Task kinds the model is strong at, each with a 0.0..=1.0 affinity.
    pub task_affinity: HashMap<String, f32>,
    /// Approx. prompt price per 1M tokens (USD); `None` = unknown/open-weight.
    pub prompt_price_per_mtok: Option<f64>,
    /// Approx. completion price per 1M tokens (USD).
    pub completion_price_per_mtok: Option<f64>,
    /// True when the entry came from live discovery rather than the built-in
    /// catalog (discovered models get conservative default metadata).
    pub discovered: bool,
}

impl ModelInfo {
    /// Affinity for a task kind (0.0 when absent).
    pub fn affinity(&self, kind: TaskKind) -> f32 {
        self.task_affinity
            .get(kind.as_str())
            .copied()
            .unwrap_or(0.0)
    }
}

/// A mutable catalog: built-in models merged with discovered models, plus
/// per-model runtime stats (latency, error rate) learned by the router.
#[derive(Debug, Default)]
pub struct ModelCatalog {
    models: RwLock<HashMap<String, ModelInfo>>,
    /// id -> (ema_latency_ms, ema_error_rate) learned at runtime.
    stats: RwLock<HashMap<String, (f64, f64)>>,
}

impl ModelCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// A catalog pre-seeded with the built-in model metadata.
    pub fn with_builtins() -> Self {
        let catalog = Self::default();
        {
            let mut guard = catalog.models.write().unwrap();
            for model in builtin_models() {
                guard.insert(model.id.clone(), model);
            }
        }
        catalog
    }

    /// Insert or replace a model. A discovered model does not overwrite a
    /// built-in entry for the same id (built-in metadata is richer); the
    /// built-in entry is merely flagged live/available.
    pub fn upsert(&self, model: ModelInfo) {
        let mut guard = self.models.write().unwrap();
        let id = model.id.clone();
        match guard.get_mut(&id) {
            Some(existing) if !existing.discovered && model.discovered => {
                existing.discovered = true;
            }
            Some(existing) => *existing = model,
            None => {
                guard.insert(id, model);
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<ModelInfo> {
        self.models.read().unwrap().get(id).cloned()
    }

    pub fn remove(&self, id: &str) {
        self.models.write().unwrap().remove(id);
    }

    /// All models (built-in + discovered), sorted by id for stable UI order.
    pub fn all(&self) -> Vec<ModelInfo> {
        let mut v: Vec<ModelInfo> = self.models.read().unwrap().values().cloned().collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    }

    /// Models available for a single provider.
    pub fn for_provider(&self, provider: &str) -> Vec<ModelInfo> {
        self.all()
            .into_iter()
            .filter(|m| m.provider == provider)
            .collect()
    }

    /// Record an observed completion latency so the router can prefer faster
    /// models over time (exponential moving average).
    pub fn record_latency(&self, id: &str, latency_ms: f64, errored: bool) {
        const ALPHA: f64 = 0.3;
        let mut guard = self.stats.write().unwrap();
        let entry = guard
            .entry(id.to_string())
            .or_insert((latency_ms, if errored { 1.0 } else { 0.0 }));
        entry.0 = entry.0 * (1.0 - ALPHA) + latency_ms * ALPHA;
        entry.1 = entry.1 * (1.0 - ALPHA) + (if errored { 1.0 } else { 0.0 }) * ALPHA;
    }

    /// Learned (ema_latency_ms, ema_error_rate) for a model, if observed.
    pub fn stats_for(&self, id: &str) -> Option<(f64, f64)> {
        self.stats.read().unwrap().get(id).copied()
    }

    /// Borrow all models under a read lock (avoids cloning for hot paths).
    pub fn read(&self) -> RwLockReadGuard<'_, HashMap<String, ModelInfo>> {
        self.models.read().unwrap()
    }
}

// ---------------------------------------------------------------------------
// Built-in catalog
// ---------------------------------------------------------------------------

fn affinity(pairs: &[(&str, f32)]) -> HashMap<String, f32> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

fn model(
    provider: &str,
    model_id: &str,
    name: &str,
    family: &str,
    context_window: u32,
    max_output_tokens: u32,
    supports_tools: bool,
    supports_structured_output: bool,
    supports_reasoning: bool,
    cost_tier: CostTier,
    description: &str,
    strengths: &[&str],
    task_affinity: &[(&str, f32)],
    prompt_price: Option<f64>,
    completion_price: Option<f64>,
) -> ModelInfo {
    let id = format!("{}/{}", provider, model_id);
    ModelInfo {
        id,
        provider: provider.to_string(),
        model_id: model_id.to_string(),
        name: name.to_string(),
        family: family.to_string(),
        context_window,
        max_output_tokens,
        input_modalities: vec![Modality::Text],
        supports_tools,
        supports_structured_output,
        supports_reasoning,
        cost_tier,
        description: description.to_string(),
        strengths: strengths.iter().map(|s| s.to_string()).collect(),
        task_affinity: affinity(task_affinity),
        prompt_price_per_mtok: prompt_price,
        completion_price_per_mtok: completion_price,
        discovered: false,
    }
}

/// Conservative default for a model we know the id of but have no metadata for.
pub fn discovered_default(provider: &str, model_id: &str) -> ModelInfo {
    ModelInfo {
        id: format!("{}/{}", provider, model_id),
        provider: provider.to_string(),
        model_id: model_id.to_string(),
        name: model_id.to_string(),
        family: "unknown".to_string(),
        context_window: 8192,
        max_output_tokens: 4096,
        input_modalities: vec![Modality::Text],
        supports_tools: true,
        supports_structured_output: false,
        supports_reasoning: false,
        cost_tier: CostTier::Medium,
        description: format!("Discovered model '{}' from {}.", model_id, provider),
        strengths: vec!["Auto-discovered".to_string()],
        task_affinity: affinity(&[
            ("coding", 0.5),
            ("reasoning", 0.5),
            ("other", 0.6),
        ]),
        prompt_price_per_mtok: None,
        completion_price_per_mtok: None,
        discovered: true,
    }
}

/// The built-in model catalog. New models are added here with full metadata.
pub fn builtin_models() -> Vec<ModelInfo> {
    let mut v: Vec<ModelInfo> = vec![
        // ---- OpenAI ----
        model(
            "openai",
            "gpt-4o",
            "GPT-4o",
            "gpt",
            128_000,
            16_384,
            true,
            true,
            false,
            CostTier::High,
            "General-purpose flagship with strong reasoning, coding, and vision.",
            &["Versatile", "Strong coding", "Multimodal", "Reliable tool use"],
            &[
                ("coding", 0.9),
                ("reasoning", 0.9),
                ("planning", 0.8),
                ("review", 0.85),
                ("debugging", 0.8),
                ("generation", 0.8),
                ("documentation", 0.8),
                ("architecture", 0.8),
                ("other", 0.85),
            ],
            Some(2.5),
            Some(10.0),
        ),
        model(
            "openai",
            "gpt-4o-mini",
            "GPT-4o mini",
            "gpt",
            128_000,
            16_384,
            true,
            true,
            false,
            CostTier::Low,
            "Fast, cheap GPT-4o variant for high-volume routine tasks.",
            &["Fast", "Cheap", "Good enough for routine work"],
            &[
                ("coding", 0.7),
                ("summarization", 0.8),
                ("extraction", 0.75),
                ("classification", 0.8),
                ("documentation", 0.7),
                ("other", 0.7),
            ],
            Some(0.15),
            Some(0.6),
        ),
        model(
            "openai",
            "o3",
            "o3",
            "o-series",
            200_000,
            50_000,
            true,
            true,
            true,
            CostTier::Premium,
            "Deep-reasoning model for hard problems, math, and complex code.",
            &["Deep reasoning", "Hard problems", "Strong on long tasks"],
            &[
                ("reasoning", 0.98),
                ("architecture", 0.95),
                ("debugging", 0.92),
                ("planning", 0.92),
                ("coding", 0.9),
                ("review", 0.9),
            ],
            Some(10.0),
            Some(40.0),
        ),
        // ---- Anthropic ----
        model(
            "anthropic",
            "claude-sonnet-4-5",
            "Claude Sonnet 4.5",
            "claude",
            200_000,
            64_000,
            true,
            true,
            true,
            CostTier::High,
            "Excellent agentic coding, long-horizon tool use, and code review.",
            &["Agentic coding", "Long context", "Strong tool use", "Great reviewer"],
            &[
                ("coding", 0.95),
                ("refactoring", 0.95),
                ("review", 0.95),
                ("planning", 0.9),
                ("architecture", 0.9),
                ("debugging", 0.9),
                ("reasoning", 0.88),
            ],
            Some(3.0),
            Some(15.0),
        ),
        model(
            "anthropic",
            "claude-haiku-4-5",
            "Claude Haiku 4.5",
            "claude",
            200_000,
            64_000,
            true,
            true,
            false,
            CostTier::Low,
            "Fast, affordable Claude for high-throughput coding and extraction.",
            &["Fast", "Cheap", "Solid coding", "Long context"],
            &[
                ("coding", 0.8),
                ("extraction", 0.8),
                ("summarization", 0.8),
                ("classification", 0.85),
                ("documentation", 0.78),
                ("other", 0.78),
            ],
            Some(1.0),
            Some(5.0),
        ),
        // ---- Google ----
        model(
            "google",
            "gemini-2.5-pro",
            "Gemini 2.5 Pro",
            "gemini",
            1_048_576,
            65_536,
            true,
            true,
            true,
            CostTier::High,
            "Huge context, strong reasoning, multimodal flagship.",
            &["Massive context", "Reasoning", "Multimodal"],
            &[
                ("reasoning", 0.92),
                ("coding", 0.9),
                ("architecture", 0.88),
                ("summarization", 0.9),
                ("extraction", 0.85),
                ("planning", 0.88),
            ],
            Some(1.25),
            Some(10.0),
        ),
        model(
            "google",
            "gemini-2.5-flash",
            "Gemini 2.5 Flash",
            "gemini",
            1_048_576,
            65_536,
            true,
            true,
            true,
            CostTier::Low,
            "Fast, cheap, huge-context Gemini for volume work.",
            &["Fast", "Cheap", "Huge context"],
            &[
                ("coding", 0.75),
                ("summarization", 0.85),
                ("extraction", 0.8),
                ("classification", 0.82),
                ("other", 0.75),
            ],
            Some(0.3),
            Some(2.5),
        ),
        // ---- OpenRouter (aggregator; sample of common models) ----
        model(
            "openrouter",
            "deepseek/deepseek-chat",
            "DeepSeek V3",
            "deepseek",
            65_536,
            8_192,
            true,
            true,
            true,
            CostTier::Low,
            "Strong open-weight reasoning + coding at very low cost.",
            &["Cheap", "Strong coding", "Good reasoning"],
            &[
                ("coding", 0.85),
                ("reasoning", 0.85),
                ("debugging", 0.8),
                ("refactoring", 0.8),
            ],
            Some(0.3),
            Some(1.2),
        ),
        model(
            "openrouter",
            "meta-llama/llama-3.1-405b-instruct",
            "Llama 3.1 405B",
            "llama",
            131_072,
            8_192,
            true,
            false,
            false,
            CostTier::Medium,
            "Large open-weight generalist via OpenRouter.",
            &["Open weights", "Generalist"],
            &[("coding", 0.7), ("reasoning", 0.7), ("other", 0.72)],
            Some(0.7),
            Some(1.0),
        ),
    ];

    // Local OmniRoute gateway placeholder; discovery fills in the real list.
    v.push(discovered_default("omniroute", "azure-ai/gpt-6-sol"));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_models_have_valid_ids_and_affinities() {
        let models = builtin_models();
        assert!(!models.is_empty());
        for m in &models {
            assert!(!m.id.is_empty());
            assert!(m.id.contains('/'), "id must be provider/model_id: {}", m.id);
            assert_eq!(m.id, format!("{}/{}", m.provider, m.model_id));
            assert!(m.context_window > 0);
            assert!(m.max_output_tokens > 0);
            for (_, v) in m.task_affinity.iter() {
                assert!((0.0..=1.0).contains(v), "affinity out of range in {}", m.id);
            }
        }
    }

    #[test]
    fn task_kind_round_trips() {
        for k in TaskKind::ALL {
            let parsed = TaskKind::parse(k.as_str());
            assert_eq!(parsed, Some(k), "parse failed for {:?}", k);
        }
        assert_eq!(TaskKind::parse("Code"), Some(TaskKind::Coding));
        assert_eq!(TaskKind::parse("code-review"), Some(TaskKind::Review));
        assert_eq!(TaskKind::parse("nonsense"), None);
    }

    #[test]
    fn catalog_upsert_and_lookup() {
        let catalog = ModelCatalog::new();
        let m = discovered_default("openai", "brand-new-model");
        catalog.upsert(m.clone());
        assert_eq!(catalog.get("openai/brand-new-model").unwrap().name, "brand-new-model");
        assert_eq!(catalog.all().len(), 1);
        assert_eq!(catalog.for_provider("openai").len(), 1);
        assert_eq!(catalog.for_provider("anthropic").len(), 0);
    }

    #[test]
    fn builtins_not_clobbered_by_discovery() {
        let catalog = ModelCatalog::with_builtins();
        let before = catalog.get("openai/gpt-4o").unwrap().context_window;
        catalog.upsert(discovered_default("openai", "gpt-4o"));
        let after = catalog.get("openai/gpt-4o").unwrap();
        assert_eq!(after.context_window, before);
        assert!(after.discovered, "should be flagged as live/available");
    }

    #[test]
    fn latency_stats_ema() {
        let catalog = ModelCatalog::new();
        catalog.record_latency("m", 100.0, false);
        catalog.record_latency("m", 300.0, false);
        // EMA with alpha=0.3: 100*0.7 + 300*0.3 = 160.
        let (lat, err) = catalog.stats_for("m").unwrap();
        assert!((lat - 160.0).abs() < 1.0);
        assert_eq!(err, 0.0);
        assert!(catalog.stats_for("missing").is_none());
    }
}
