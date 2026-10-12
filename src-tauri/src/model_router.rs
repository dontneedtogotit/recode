//! Weighted model router.
//!
//! Given a task description and the set of models the user has selected, the
//! router scores every candidate from the [`ModelCatalog`] and returns them
//! ranked. Scoring combines:
//!
//!   - hard capability gates (context window, modalities, tool / structured
//!     output / reasoning support) — a model that can't do the job is
//!     disqualified, not just penalised;
//!   - task-kind affinity (0.0..=1.0 from the catalog metadata);
//!   - cost tier (cheaper preferred, weighted);
//!   - learned reliability (lower observed error rate preferred);
//!   - learned speed (lower observed latency preferred);
//!
//! The router is a pure function over the catalog (no network), so it is fully
//! unit-testable offline and can be re-ranked as runtime stats accumulate.
//!
//! Synchronous to match the crate.
use crate::model_catalog::{ModelCatalog, ModelInfo, TaskKind};

/// A routing request: everything the task needs from a model.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRequest {
    /// Primary task kind the model should excel at.
    pub task_kind: TaskKind,
    /// Minimum context window (tokens) the task will consume.
    pub min_context_window: u32,
    /// Required input modalities (e.g. image for a screenshot task).
    pub required_modalities: Vec<crate::model_catalog::Modality>,
    /// Task needs tool / function calling.
    pub requires_tools: bool,
    /// Task needs reliable structured (JSON-schema) output.
    pub requires_structured_output: bool,
    /// Task benefits from a reasoning/thinking budget.
    pub prefers_reasoning: bool,
    /// Optional explicit model id — if set and eligible, it is forced to the
    /// top regardless of score (user override wins).
    pub preferred_model: Option<String>,
}

impl Default for RoutingRequest {
    fn default() -> Self {
        Self {
            task_kind: TaskKind::Other,
            min_context_window: 0,
            required_modalities: Vec::new(),
            requires_tools: false,
            requires_structured_output: false,
            prefers_reasoning: false,
            preferred_model: None,
        }
    }
}

impl RoutingRequest {
    /// Parse a free-form task kind label, defaulting to `Other`.
    pub fn with_task_kind(mut self, kind: &str) -> Self {
        if let Some(k) = TaskKind::parse(kind) {
            self.task_kind = k;
        }
        self
    }
}

/// One candidate's score breakdown, for transparency in the UI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateScore {
    pub model_id: String,
    pub provider: String,
    pub model: String,
    pub eligible: bool,
    /// Why the model was disqualified (present only when `eligible == false`).
    pub disqualification: Option<String>,
    pub score: f64,
    pub affinity: f64,
    pub cost_penalty: f64,
    pub reliability: f64,
    pub speed: f64,
}

/// The router's decision: ranked candidates plus the top pick.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingDecision {
    pub chosen: Option<CandidateScore>,
    pub ranked: Vec<CandidateScore>,
    pub considered: usize,
}

/// Weight knobs. Defaults are sensible; tune per deployment if needed.
#[derive(Debug, Clone)]
pub struct RouterWeights {
    pub affinity: f64,
    pub cost: f64,
    pub reliability: f64,
    pub speed: f64,
    pub reasoning_bonus: f64,
    pub affinity_bonus: f64,
}

impl Default for RouterWeights {
    fn default() -> Self {
        Self {
            affinity: 0.50,
            cost: 0.15,
            reliability: 0.20,
            speed: 0.10,
            reasoning_bonus: 0.05,
            affinity_bonus: 0.10,
        }
    }
}

/// Score a single model against a request. Pure; no side effects.
fn score_model(
    model: &ModelInfo,
    request: &RoutingRequest,
    weights: &RouterWeights,
    stats: Option<(f64, f64)>, // (ema_latency_ms, ema_error_rate)
) -> CandidateScore {
    let base = CandidateScore {
        model_id: model.id.clone(),
        provider: model.provider.clone(),
        model: model.model_id.clone(),
        eligible: true,
        disqualification: None,
        score: 0.0,
        affinity: 0.0,
        cost_penalty: 0.0,
        reliability: 0.0,
        speed: 0.0,
    };

    // --- Hard capability gates ---
    if model.context_window < request.min_context_window {
        return disqualified(
            base,
            format!(
                "context window {} < required {}",
                model.context_window, request.min_context_window
            ),
        );
    }
    for required in &request.required_modalities {
        if !model.input_modalities.contains(required) {
            return disqualified(
                base,
                format!("missing input modality {}", required.as_str()),
            );
        }
    }
    if request.requires_tools && !model.supports_tools {
        return disqualified(base, "does not support tool calling".into());
    }
    if request.requires_structured_output && !model.supports_structured_output {
        return disqualified(
            base,
            "does not support reliable structured output".into(),
        );
    }

    // --- Task affinity ---
    let affinity = model.affinity(request.task_kind) as f64;

    // --- Cost (cheaper is better). Map tier weight to a 0..=1 preference. ---
    let tier_weight = model.cost_tier.weight() as f64; // 0..=10
    let cost_pref = (1.0 - tier_weight / 10.0).clamp(0.0, 1.0);

    // --- Reliability + speed from learned stats (default to neutral 0.5). ---
    let (reliability, speed) = match stats {
        Some((latency_ms, error_rate)) => {
            // error_rate 0.0..=1.0 -> reliability 1.0..=0.0
            let reliability = (1.0 - error_rate.clamp(0.0, 1.0)).clamp(0.0, 1.0);
            // latency: assume ~1200ms is "fast" and ~8000ms is "slow".
            let speed = ((8000.0 - latency_ms.clamp(0.0, 8000.0)) / 7000.0).clamp(0.0, 1.0);
            (reliability, speed)
        }
        None => (0.5, 0.5),
    };

    // --- Combine ---
    let mut score = weights.affinity * affinity
        + weights.cost * cost_pref
        + weights.reliability * reliability
        + weights.speed * speed;
    if request.prefers_reasoning && model.supports_reasoning {
        score += weights.reasoning_bonus;
    }
    // A small boost when affinity is high, rewarding a clear best-fit.
    if affinity >= 0.8 {
        score += weights.affinity_bonus;
    }
    score = score.clamp(0.0, 1.0);

    CandidateScore {
        score,
        affinity,
        cost_penalty: (1.0 - cost_pref),
        reliability,
        speed,
        ..base
    }
}

fn disqualified(mut s: CandidateScore, reason: String) -> CandidateScore {
    s.eligible = false;
    s.disqualification = Some(reason);
    s.score = 0.0;
    s
}

/// The router. Stateless; holds only tunable weights.
#[derive(Debug, Clone, Default)]
pub struct ModelRouter {
    weights: RouterWeights,
}

impl ModelRouter {
    pub fn new() -> Self {
        Self {
            weights: RouterWeights::default(),
        }
    }

    pub fn with_weights(mut self, weights: RouterWeights) -> Self {
        self.weights = weights;
        self
    }

    /// Rank `candidates` for `request`. If `preferred_model` is set and
    /// eligible, it is promoted to the top.
    pub fn route(&self, candidates: &[ModelInfo], request: &RoutingRequest) -> RoutingDecision {
        let mut scores: Vec<CandidateScore> = candidates
            .iter()
            .map(|m| score_model(m, request, &self.weights, None))
            .collect();

        // Attach learned stats when available (caller may re-score below).
        let considered = scores.len();
        scores.sort_by(|a, b| b.score.total_cmp(&a.score));

        let chosen = if let Some(pref) = &request.preferred_model {
            // Find eligible preferred; promote to top if present.
            match scores.iter().position(|s| &s.model_id == pref && s.eligible) {
                Some(idx) => {
                    let mut chosen = scores.remove(idx);
                    scores.insert(0, chosen.clone());
                    Some(chosen)
                }
                None => scores.first().filter(|s| s.eligible).cloned(),
            }
        } else {
            scores.first().filter(|s| s.eligible).cloned()
        };

        RoutingDecision {
            chosen,
            ranked: scores,
            considered,
        }
    }

    /// Like `route`, but consults live learned stats from `catalog`.
    pub fn route_live(
        &self,
        catalog: &ModelCatalog,
        request: &RoutingRequest,
    ) -> RoutingDecision {
        let candidates = catalog.all();
        let mut scores: Vec<CandidateScore> = candidates
            .iter()
            .map(|m| {
                let stats = catalog.stats_for(&m.id);
                score_model(m, request, &self.weights, stats)
            })
            .collect();

        let considered = scores.len();
        scores.sort_by(|a, b| b.score.total_cmp(&a.score));

        let chosen = if let Some(pref) = &request.preferred_model {
            match scores.iter().position(|s| &s.model_id == pref && s.eligible) {
                Some(idx) => {
                    let mut chosen = scores.remove(idx);
                    scores.insert(0, chosen.clone());
                    Some(chosen)
                }
                None => scores.first().filter(|s| s.eligible).cloned(),
            }
        } else {
            scores.first().filter(|s| s.eligible).cloned()
        };

        RoutingDecision {
            chosen,
            ranked: scores,
            considered,
        }
    }
}

/// Infer a [`RoutingRequest`] from a free-form task description. Heuristic,
/// no LLM: keyword-based task-kind detection + conservative capability needs.
pub fn infer_request(description: &str) -> RoutingRequest {
    let lower = description.to_lowercase();
    let mut request = RoutingRequest::default();

    // Task kind (first match wins).
    if lower.contains("plan") || lower.contains("architect") {
        request.task_kind = TaskKind::Planning;
    } else if lower.contains("refactor") {
        request.task_kind = TaskKind::Refactoring;
    } else if lower.contains("test") || lower.contains("qa") {
        request.task_kind = TaskKind::Testing;
    } else if lower.contains("review") {
        request.task_kind = TaskKind::Review;
    } else if lower.contains("debug") || lower.contains("diagnos") {
        request.task_kind = TaskKind::Debugging;
    } else if lower.contains("document") || lower.contains("readme") || lower.contains("docs") {
        request.task_kind = TaskKind::Documentation;
    } else if lower.contains("explain") || lower.contains("reason") || lower.contains("analy") {
        request.task_kind = TaskKind::Reasoning;
    } else if lower.contains("summar") {
        request.task_kind = TaskKind::Summarization;
    } else if lower.contains("translate") {
        request.task_kind = TaskKind::Translation;
    } else if lower.contains("extract") || lower.contains("structured") || lower.contains("json") {
        request.task_kind = TaskKind::Extraction;
        request.requires_structured_output = true;
    } else if lower.contains("code")
        || lower.contains("implement")
        || lower.contains("function")
        || lower.contains("api")
        || lower.contains("build")
    {
        request.task_kind = TaskKind::Coding;
        request.requires_tools = true;
    } else {
        request.task_kind = TaskKind::Other;
    }

    // Context: long-doc / repo-level tasks need more room.
    if lower.contains("repo")
        || lower.contains("codebase")
        || lower.contains("entire")
        || lower.contains("large")
        || lower.contains("long")
    {
        request.min_context_window = 64_000;
    }

    // Vision: images / screenshots.
    if lower.contains("image")
        || lower.contains("screenshot")
        || lower.contains("picture")
        || lower.contains("diagram")
        || lower.contains("visual")
    {
        request
            .required_modalities
            .push(crate::model_catalog::Modality::Image);
    }

    request
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_catalog::CostTier;

    fn coding_model(id: &str, affinity: f32, tier: CostTier, tools: bool, ctx: u32) -> ModelInfo {
        ModelInfo {
            id: id.into(),
            provider: "openai".into(),
            model_id: id.split('/').nth(1).unwrap_or(id).into(),
            name: id.into(),
            family: "gpt".into(),
            context_window: ctx,
            max_output_tokens: 8192,
            input_modalities: vec![crate::model_catalog::Modality::Text],
            supports_tools: tools,
            supports_structured_output: true,
            supports_reasoning: false,
            cost_tier: tier,
            description: String::new(),
            strengths: vec![],
            task_affinity: std::collections::HashMap::from([("coding".to_string(), affinity)]),
            prompt_price_per_mtok: None,
            completion_price_per_mtok: None,
            discovered: false,
        }
    }

    #[test]
    fn prefers_high_affinity_model() {
        let router = ModelRouter::new();
        let candidates = vec![
            coding_model("openai/weak", 0.3, CostTier::Low, true, 128_000),
            coding_model("openai/strong", 0.95, CostTier::High, true, 200_000),
        ];
        let req = RoutingRequest {
            task_kind: TaskKind::Coding,
            requires_tools: true,
            ..Default::default()
        };
        let decision = router.route(&candidates, &req);
        assert!(decision.chosen.is_some());
        assert_eq!(decision.chosen.unwrap().model_id, "openai/strong");
    }

    #[test]
    fn disqualifies_when_context_too_small() {
        let router = ModelRouter::new();
        let candidates = vec![coding_model("openai/small", 0.95, CostTier::Low, true, 8_000)];
        let req = RoutingRequest {
            task_kind: TaskKind::Coding,
            min_context_window: 100_000,
            ..Default::default()
        };
        let decision = router.route(&candidates, &req);
        assert!(decision.chosen.is_none());
        assert!(!decision.ranked[0].eligible);
        assert!(decision.ranked[0]
            .disqualification
            .as_deref()
            .unwrap()
            .contains("context window"));
    }

    #[test]
    fn disqualifies_missing_tools() {
        let router = ModelRouter::new();
        let candidates = vec![coding_model("openai/notools", 0.95, CostTier::Low, false, 128_000)];
        let req = RoutingRequest {
            task_kind: TaskKind::Coding,
            requires_tools: true,
            ..Default::default()
        };
        let decision = router.route(&candidates, &req);
        assert!(decision.chosen.is_none());
    }

    #[test]
    fn preferred_model_is_promoted_when_eligible() {
        let router = ModelRouter::new();
        let candidates = vec![
            coding_model("openai/weak", 0.3, CostTier::Low, true, 128_000),
            coding_model("openai/strong", 0.95, CostTier::High, true, 200_000),
        ];
        let mut req = RoutingRequest {
            task_kind: TaskKind::Coding,
            requires_tools: true,
            ..Default::default()
        };
        req.preferred_model = Some("openai/weak".into());
        let decision = router.route(&candidates, &req);
        assert_eq!(
            decision.chosen.unwrap().model_id,
            "openai/weak",
            "user override wins over raw score"
        );
    }

    #[test]
    fn preferred_model_that_is_ineligible_falls_back() {
        let router = ModelRouter::new();
        let candidates = vec![
            coding_model("openai/weak", 0.3, CostTier::Low, false, 128_000), // no tools
            coding_model("openai/strong", 0.95, CostTier::High, true, 200_000),
        ];
        let mut req = RoutingRequest {
            task_kind: TaskKind::Coding,
            requires_tools: true,
            ..Default::default()
        };
        req.preferred_model = Some("openai/weak".into());
        let decision = router.route(&candidates, &req);
        // weak is ineligible (no tools) so strong should win.
        assert_eq!(
            decision.chosen.unwrap().model_id,
            "openai/strong"
        );
    }

    #[test]
    fn infer_request_detects_coding_and_tools() {
        let req = infer_request("Implement a REST API function");
        assert_eq!(req.task_kind, TaskKind::Coding);
        assert!(req.requires_tools);
    }

    #[test]
    fn infer_request_detects_vision() {
        let req = infer_request("Explain this screenshot of the UI");
        assert!(req
            .required_modalities
            .contains(&crate::model_catalog::Modality::Image));
    }

    #[test]
    fn route_live_consults_stats() {
        let catalog = ModelCatalog::new();
        let weak = coding_model("openai/weak", 0.5, CostTier::Low, true, 128_000);
        let strong = coding_model("openai/strong", 0.5, CostTier::Low, true, 128_000);
        catalog.upsert(weak);
        catalog.upsert(strong);
        // strong is slow + error-prone; weak is fast + reliable.
        catalog.record_latency("openai/strong", 9000.0, true);
        catalog.record_latency("openai/weak", 300.0, false);
        let router = ModelRouter::new();
        let req = RoutingRequest {
            task_kind: TaskKind::Coding,
            requires_tools: true,
            ..Default::default()
        };
        let decision = router.route_live(&catalog, &req);
        // equal affinity + cost, so learned speed/reliability decides.
        assert_eq!(
            decision.chosen.unwrap().model_id,
            "openai/weak"
        );
    }
}
