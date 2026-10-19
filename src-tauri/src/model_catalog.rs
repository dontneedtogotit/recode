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
use crate::domain::DomainError;

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
            let mut guard = catalog.models.write().unwrap_or_else(std::sync::PoisonError::into_inner);
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
        let mut guard = self.models.write().unwrap_or_else(std::sync::PoisonError::into_inner);
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
        self.models.read().unwrap_or_else(std::sync::PoisonError::into_inner).get(id).cloned()
    }

    pub fn remove(&self, id: &str) {
        self.models.write().unwrap_or_else(std::sync::PoisonError::into_inner).remove(id);
    }

    /// All models (built-in + discovered), sorted by id for stable UI order.
    pub fn all(&self) -> Vec<ModelInfo> {
        let mut v: Vec<ModelInfo> = self.models.read().unwrap_or_else(std::sync::PoisonError::into_inner).values().cloned().collect();
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
        let mut guard = self.stats.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        let entry = guard
            .entry(id.to_string())
            .or_insert((latency_ms, if errored { 1.0 } else { 0.0 }));
        entry.0 = entry.0 * (1.0 - ALPHA) + latency_ms * ALPHA;
        entry.1 = entry.1 * (1.0 - ALPHA) + (if errored { 1.0 } else { 0.0 }) * ALPHA;
    }

    /// Learned (ema_latency_ms, ema_error_rate) for a model, if observed.
    pub fn stats_for(&self, id: &str) -> Option<(f64, f64)> {
        self.stats.read().unwrap_or_else(std::sync::PoisonError::into_inner).get(id).copied()
    }

    /// Borrow all models under a read lock (avoids cloning for hot paths).
    pub fn read(&self) -> RwLockReadGuard<'_, HashMap<String, ModelInfo>> {
        self.models.read().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Persist the entire catalog (built-in + discovered + user-edited models)
    /// as a single JSON row so it survives a restart. Call after any mutation
    /// that outlives the process (upsert / remove / discovery). The DB is
    /// opened via a separate connection by the caller, so this is safe to run
    /// while `AppState` holds its own connection.
    pub fn save(&self, conn: &rusqlite::Connection) -> Result<(), DomainError> {
        let json = serde_json::to_string(&self.all())
            .map_err(|e| DomainError::internal(format!("serialize model catalog failed: {e}")))?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS model_catalog (id INTEGER PRIMARY KEY CHECK (id = 1), json TEXT NOT NULL)")
            .map_err(|e| DomainError::internal(format!("model catalog schema creation failed: {e}")))?;
        conn.execute(
            "INSERT INTO model_catalog (id, json) VALUES (1, ?1) ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            rusqlite::params![json],
        )
        .map_err(|e| DomainError::internal(format!("model catalog persist failed: {e}")))?;
        Ok(())
    }

    /// Restore a previously persisted catalog into `self`, layering saved
    /// entries over the built-ins. No-op when no row exists (fresh install) or
    /// the stored payload is corrupt.
    pub fn load(&self, conn: &rusqlite::Connection) -> Result<usize, DomainError> {
        let row: Option<String> = conn
            .query_row("SELECT json FROM model_catalog WHERE id = 1", [], |row| row.get(0))
            .ok();
        let Some(json) = row else {
            return Ok(0);
        };
        let saved: Vec<ModelInfo> = serde_json::from_str(&json)
            .map_err(|e| DomainError::internal(format!("deserialize model catalog failed: {e}")))?;
        let mut n = 0;
        for model in saved {
            self.upsert(model);
            n += 1;
        }
        Ok(n)
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

/// Inferred capabilities for a discovered model, keyed off its id. This is
/// what makes each auto-discovered model carry *individual* metadata rather
/// than one shared stub: the id is pattern-matched against known families to
/// pick a realistic context window, output cap, and capability flags.
struct Inferred {
    family: &'static str,
    context_window: u32,
    max_output: u32,
    vision: bool,
    reasoning: bool,
    tools: bool,
    structured: bool,
    cost: CostTier,
    /// One-line capability summary folded into the model's description.
    summary: &'static str,
    strengths: &'static [&'static str],
    affinity: &'static [(&'static str, f32)],
}

/// Pattern-match a model id (with its provider as a tie-breaker) to the most
/// specific known family. Order matters: specific prefixes first, generic
/// substrings later, unknown ids fall through to a conservative default.
fn infer_family(provider: &str, model_id: &str) -> Inferred {
    let m = model_id.to_lowercase();
    let p = provider.to_lowercase();

    // --- OpenAI: reasoning (o-series) ---
    if p == "openai" && (m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") || m.contains("-o1") || m.contains("-o3")) {
        return Inferred {
            family: "gpt-o",
            context_window: 128_000,
            max_output: 32_000,
            vision: true,
            reasoning: true,
            tools: true,
            structured: true,
            cost: CostTier::High,
            summary: "a long-chain reasoning model that thinks step-by-step before answering",
            strengths: &["Deep reasoning", "Complex planning", "Strong coding", "Vision"],
            affinity: &[( "reasoning", 0.95), ("coding", 0.9), ("planning", 0.9), ("debugging", 0.9), ("review", 0.85), ("other", 0.7)],
        };
    }
    // --- OpenAI: GPT-4o / GPT-4.1 / GPT-4 family ---
    if p == "openai"
        && (m.contains("gpt-4o") || m.contains("gpt-4.1") || m.contains("gpt-4") || m.contains("gpt-3.5"))
    {
        let mini = m.contains("mini") || m.contains("nano");
        return Inferred {
            family: "gpt",
            context_window: 128_000,
            max_output: if mini { 16_384 } else { 16_384 },
            vision: !m.contains("gpt-3.5"),
            reasoning: false,
            tools: true,
            structured: true,
            cost: if mini { CostTier::Low } else { CostTier::High },
            summary: if mini {
                "a fast, low-cost generalist for high-volume text, chat, and simple tool calls"
            } else {
                "a general-purpose flagship with strong reasoning, coding, and vision"
            },
            strengths: &["Versatile", "Strong coding", "Multimodal", "Reliable tool use"],
            affinity: &[("coding", 0.9), ("reasoning", 0.85), ("planning", 0.8), ("review", 0.85), ("generation", 0.85), ("documentation", 0.85), ("other", 0.85)],
        };
    }
    // --- Anthropic: Claude ---
    if p == "anthropic" || m.contains("claude") {
        let opus = m.contains("opus");
        let haiku = m.contains("haiku");
        let vision = !haiku; // haiku historically text-only
        return Inferred {
            family: "claude",
            context_window: 200_000,
            max_output: 8_192,
            vision,
            reasoning: opus,
            tools: true,
            structured: true,
            cost: if opus { CostTier::High } else if haiku { CostTier::Low } else { CostTier::Medium },
            summary: if opus {
                "a top-tier reasoning model built for hard, long-context analysis and code"
            } else if haiku {
                "a fast, inexpensive text model for lightweight chat and extraction"
            } else {
                "a balanced long-context model strong at writing, summarization, and code"
            },
            strengths: if opus {
                &["Top reasoning", "200K context", "Strong code", "Careful tool use"]
            } else {
                &["Long context", "Strong writing", "Good code", "Reliable tool use"]
            },
            affinity: &[("reasoning", 0.9), ("coding", 0.85), ("documentation", 0.9), ("review", 0.85), ("planning", 0.8), ("generation", 0.85), ("other", 0.85)],
        };
    }
    // --- Google: Gemini ---
    if m.contains("gemini") || p == "google" {
        let flash = m.contains("flash");
        let lite = m.contains("lite");
        let pro = m.contains("pro") || (!flash && !lite);
        return Inferred {
            family: "gemini",
            context_window: if pro { 1_000_000 } else { 1_000_000 },
            max_output: if pro { 8_192 } else { 8_192 },
            vision: true,
            reasoning: pro,
            tools: true,
            structured: true,
            cost: if lite { CostTier::Low } else if flash { CostTier::Low } else { CostTier::High },
            summary: if flash || lite {
                "a fast, cheap multimodal model tuned for high-throughput text and vision"
            } else {
                "a large-context multimodal model for long documents, code, and reasoning"
            },
            strengths: &["1M context", "Multimodal", "Strong reasoning", "Good code"],
            affinity: &[("reasoning", 0.85), ("coding", 0.8), ("planning", 0.8), ("review", 0.8), ("documentation", 0.85), ("generation", 0.85), ("other", 0.85)],
        };
    }
    // --- Meta: Llama ---
    if m.contains("llama") {
        let vision = m.contains("11") || m.contains("11b") || m.contains("113b") || m.contains("-v") || m.contains("vision");
        let big = m.contains("405b") || m.contains("70b") || m.contains("3.3");
        return Inferred {
            family: "llama",
            context_window: if big { 128_000 } else { 8_192 },
            max_output: 4_096,
            vision,
            reasoning: false,
            tools: true,
            structured: false,
            cost: if big { CostTier::Medium } else { CostTier::Low },
            summary: "an open-weight general model for chat, generation, and fine-tuning",
            strengths: &["Open weights", "Fast", "Strong base reasoning", "Fine-tunable"],
            affinity: &[("generation", 0.85), ("coding", 0.7), ("reasoning", 0.7), ("documentation", 0.8), ("other", 0.8)],
        };
    }
    // --- DeepSeek: r1 (reasoning) vs chat/v3 ---
    if p == "deepseek" || m.contains("deepseek") {
        let r1 = m.contains("r1");
        let strengths: &'static [&'static str] = if r1 {
            &["Very low cost", "Strong coding", "Long context", "Reasoning"]
        } else {
            &["Very low cost", "Strong coding", "Long context", "Fast"]
        };
        let affinity: &'static [(&'static str, f32)] = if r1 {
            &[
                ("coding", 0.9),
                ("reasoning", 0.95),
                ("planning", 0.8),
                ("review", 0.8),
                ("other", 0.8),
            ]
        } else {
            &[
                ("coding", 0.9),
                ("reasoning", 0.8),
                ("planning", 0.8),
                ("review", 0.8),
                ("other", 0.8),
            ]
        };
        return Inferred {
            family: "deepseek",
            context_window: 64_000,
            max_output: 8_192,
            vision: false,
            reasoning: r1,
            tools: true,
            structured: false,
            cost: CostTier::Low,
            summary: if r1 {
                "a low-cost reasoning model that deliberates before answering, strong in math and code"
            } else {
                "a low-cost general model for chat, code, and long-document summarization"
            },
            strengths,
            affinity,
        };
    }
    // --- Mistral: codestral / mixtral / mistral ---
    if m.contains("codestral") {
        return Inferred {
            family: "mistral",
            context_window: 32_000,
            max_output: 8_192,
            vision: false,
            reasoning: false,
            tools: true,
            structured: false,
            cost: CostTier::Medium,
            summary: "a code-specialized model tuned for repository-level coding and completion",
            strengths: &["Code-specialized", "Agentic coding", "Good completion", "Reliable tool use"],
            affinity: &[("coding", 0.95), ("refactoring", 0.9), ("review", 0.9), ("debugging", 0.85), ("other", 0.7)],
        };
    }
    if p == "mistral" || m.contains("mistral") || m.contains("mixtral") {
        return Inferred {
            family: "mistral",
            context_window: 32_000,
            max_output: 8_192,
            vision: false,
            reasoning: false,
            tools: true,
            structured: false,
            cost: CostTier::Medium,
            summary: "a fast, efficient general model for chat and generation",
            strengths: &["Fast", "Efficient", "Good generation", "Reliable tool use"],
            affinity: &[("generation", 0.85), ("reasoning", 0.75), ("coding", 0.7), ("documentation", 0.8), ("other", 0.8)],
        };
    }
    // --- xAI: Grok ---
    if p == "xai" || p == "grok" || m.contains("grok") {
        return Inferred {
            family: "grok",
            context_window: 128_000,
            max_output: 8_192,
            vision: true,
            reasoning: m.contains("grok-3") || m.contains("grok-4"),
            tools: true,
            structured: true,
            cost: CostTier::High,
            summary: "a large multimodal generalist with strong reasoning and code",
            strengths: &["Multimodal", "Strong reasoning", "Strong code", "Long context"],
            affinity: &[("reasoning", 0.85), ("coding", 0.85), ("planning", 0.8), ("review", 0.8), ("other", 0.85)],
        };
    }
    // --- Alibaba: Qwen ---
    if m.contains("qwen") {
        let vision = m.contains("-v") || m.contains("vl") || m.contains("vision");
        return Inferred {
            family: "qwen",
            context_window: 32_000,
            max_output: 8_192,
            vision,
            reasoning: m.contains("qwq") || m.contains("-r1"),
            tools: true,
            structured: false,
            cost: CostTier::Low,
            summary: "a multilingual general model strong at code, reasoning, and long text",
            strengths: &["Multilingual", "Strong code", "Low cost", "Good reasoning"],
            affinity: &[("coding", 0.85), ("reasoning", 0.8), ("generation", 0.8), ("documentation", 0.8), ("other", 0.8)],
        };
    }
    // --- Conservative fallback: we saw the id but recognize no family. ---
    Inferred {
        family: "unknown",
        context_window: 8_192,
        max_output: 4_096,
        vision: false,
        reasoning: false,
        tools: true,
        structured: false,
        cost: CostTier::Medium,
        summary: "an auto-discovered model of unknown lineage; assume modest context and text-only input",
        strengths: &["Auto-discovered"],
        affinity: &[("coding", 0.5), ("reasoning", 0.5), ("other", 0.6)],
    }
}

/// Derive rich per-model metadata for a model we know the id of but have no
/// explicit catalog entry for. Instead of one flat default, this inspects the
/// model id (and provider) to infer a family, then fills in a realistic
/// context window, output cap, and capability flags plus a tailored
/// description and strengths — so each discovered model gets individual,
/// useful metadata rather than a generic stub.
pub fn discovered_default(provider: &str, model_id: &str) -> ModelInfo {
    let inferred = infer_family(provider, model_id);
    let mut modalities = vec![Modality::Text];
    if inferred.vision {
        modalities.push(Modality::Image);
    }
    ModelInfo {
        id: format!("{}/{}", provider, model_id),
        provider: provider.to_string(),
        model_id: model_id.to_string(),
        name: model_id.to_string(),
        family: inferred.family.to_string(),
        context_window: inferred.context_window,
        max_output_tokens: inferred.max_output,
        input_modalities: modalities,
        supports_tools: inferred.tools,
        supports_structured_output: inferred.structured,
        supports_reasoning: inferred.reasoning,
        cost_tier: inferred.cost,
        description: format!(
            "Auto-discovered {} model '{}' from {} — {}.",
            inferred.family, model_id, provider, inferred.summary
        ),
        strengths: inferred.strengths.iter().map(|s| s.to_string()).collect(),
        task_affinity: affinity(inferred.affinity),
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
