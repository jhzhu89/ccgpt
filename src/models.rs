use std::{collections::HashMap, error::Error, fmt};

use serde::Deserialize;

use crate::ir::ReasoningEffort;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelTier {
    High,
    Balanced,
    Fast,
}

impl ModelTier {
    fn suffix(self) -> &'static str {
        match self {
            Self::High => "sol",
            Self::Balanced => "terra",
            Self::Fast => "luna",
        }
    }
}

impl fmt::Display for ModelTier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.suffix())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelTargets {
    pub high: String,
    pub balanced: String,
    pub fast: String,
}

impl ModelTargets {
    pub fn get(&self, tier: ModelTier) -> &str {
        match tier {
            ModelTier::High => &self.high,
            ModelTier::Balanced => &self.balanced,
            ModelTier::Fast => &self.fast,
        }
    }
}

impl Default for ModelTargets {
    fn default() -> Self {
        Self {
            high: "gpt-5.6-sol".to_owned(),
            balanced: "gpt-5.6-terra".to_owned(),
            fast: "gpt-5.6-luna".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilities {
    pub supports_parallel_tool_calls: bool,
    pub reasoning_efforts: Vec<ReasoningEffort>,
    pub limits: ModelLimits,
}

impl ModelCapabilities {
    fn direct(model: &str) -> Self {
        Self {
            supports_parallel_tool_calls: true,
            reasoning_efforts: vec![
                ReasoningEffort::None,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::Xhigh,
                ReasoningEffort::Max,
            ],
            limits: ModelLimits::known(model),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
pub struct ModelLimits {
    pub max_context_window_tokens: Option<u64>,
    pub max_prompt_tokens: Option<u64>,
    pub max_output_tokens: Option<u64>,
}

impl ModelLimits {
    fn known(model: &str) -> Self {
        let model = model.to_ascii_lowercase();
        let Some(variant) = model.strip_prefix("gpt-5.6-") else {
            return Self::default();
        };
        if !["sol", "terra", "luna"].iter().any(|tier| {
            variant == *tier
                || variant
                    .strip_prefix(tier)
                    .is_some_and(|suffix| suffix.starts_with('-'))
        }) {
            return Self::default();
        }
        Self {
            max_context_window_tokens: Some(1_050_000),
            max_prompt_tokens: Some(922_000),
            max_output_tokens: Some(128_000),
        }
    }

    fn with_fallback(self, fallback: Self) -> Self {
        Self {
            max_context_window_tokens: self
                .max_context_window_tokens
                .or(fallback.max_context_window_tokens),
            max_prompt_tokens: self.max_prompt_tokens.or(fallback.max_prompt_tokens),
            max_output_tokens: self.max_output_tokens.or(fallback.max_output_tokens),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CopilotModel {
    pub id: String,
    #[serde(default)]
    pub supported_endpoints: Vec<String>,
    #[serde(default)]
    pub capabilities: CopilotCapabilities,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct CopilotCapabilities {
    #[serde(default)]
    pub supports: CopilotSupports,
    #[serde(default)]
    pub limits: ModelLimits,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct CopilotSupports {
    pub parallel_tool_calls: Option<bool>,
    #[serde(default)]
    pub reasoning_effort: Vec<String>,
}

impl CopilotModel {
    pub fn model_capabilities(&self) -> ModelCapabilities {
        ModelCapabilities {
            supports_parallel_tool_calls: self.capabilities.supports.parallel_tool_calls
                == Some(true),
            reasoning_efforts: self
                .capabilities
                .supports
                .reasoning_effort
                .iter()
                .filter_map(|effort| parse_reasoning_effort(effort))
                .collect(),
            limits: self
                .capabilities
                .limits
                .with_fallback(ModelLimits::known(&self.id)),
        }
    }

    fn supports_responses(&self) -> bool {
        self.supported_endpoints
            .iter()
            .any(|endpoint| endpoint.eq_ignore_ascii_case("/responses"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModel {
    pub model: String,
    pub capabilities: ModelCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    MissingTier(ModelTier),
    Unavailable(String),
}

impl fmt::Display for ModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTier(tier) => {
                write!(
                    formatter,
                    "Copilot has no Responses model in the {tier} tier"
                )
            }
            Self::Unavailable(model) => write!(formatter, "{model} is not available from Copilot"),
        }
    }
}

impl Error for ModelError {}

#[derive(Debug, Clone)]
enum Routing {
    Copilot {
        models: HashMap<String, ResolvedModel>,
        tiers: HashMap<ModelTier, ResolvedModel>,
    },
    Direct,
}

#[derive(Debug, Clone)]
pub struct ModelRouter {
    pub targets: ModelTargets,
    routing: Routing,
}

impl ModelRouter {
    pub fn copilot(models: impl IntoIterator<Item = CopilotModel>) -> Result<Self, ModelError> {
        let models: Vec<_> = models
            .into_iter()
            .filter(CopilotModel::supports_responses)
            .collect();
        let mut tiers = HashMap::new();
        for tier in [ModelTier::High, ModelTier::Balanced, ModelTier::Fast] {
            let selected = select_latest(&models, tier).ok_or(ModelError::MissingTier(tier))?;
            tiers.insert(tier, resolve_copilot_model(selected));
        }
        let targets = ModelTargets {
            high: tiers[&ModelTier::High].model.clone(),
            balanced: tiers[&ModelTier::Balanced].model.clone(),
            fast: tiers[&ModelTier::Fast].model.clone(),
        };
        let models = models
            .iter()
            .map(|model| (model.id.to_ascii_lowercase(), resolve_copilot_model(model)))
            .collect();
        Ok(Self {
            targets,
            routing: Routing::Copilot { models, tiers },
        })
    }

    pub fn direct(targets: ModelTargets) -> Self {
        Self {
            targets,
            routing: Routing::Direct,
        }
    }

    pub fn resolve(&self, requested: &str) -> Result<ResolvedModel, ModelError> {
        match &self.routing {
            Routing::Copilot { models, tiers } => {
                if let Some(model) = models.get(&requested.to_ascii_lowercase()) {
                    return Ok(model.clone());
                }
                if requested.to_ascii_lowercase().starts_with("gpt-") {
                    return Err(ModelError::Unavailable(requested.to_owned()));
                }
                let tier = claude_tier(requested).unwrap_or(ModelTier::High);
                Ok(tiers[&tier].clone())
            }
            Routing::Direct => {
                let exact = [
                    self.targets.get(ModelTier::High),
                    self.targets.get(ModelTier::Balanced),
                    self.targets.get(ModelTier::Fast),
                ]
                .into_iter()
                .find(|model| model.eq_ignore_ascii_case(requested));
                let model = match (exact, claude_tier(requested)) {
                    (Some(model), _) => model,
                    (None, Some(tier)) => self.targets.get(tier),
                    (None, None) => requested,
                };
                Ok(ResolvedModel {
                    model: model.to_owned(),
                    capabilities: ModelCapabilities::direct(model),
                })
            }
        }
    }

    pub fn auto_compact_tokens(&self) -> Option<u64> {
        let limits = match &self.routing {
            Routing::Copilot { tiers, .. } => [
                tiers.get(&ModelTier::High)?.capabilities.limits,
                tiers.get(&ModelTier::Balanced)?.capabilities.limits,
                tiers.get(&ModelTier::Fast)?.capabilities.limits,
            ],
            Routing::Direct => [
                ModelLimits::known(&self.targets.high),
                ModelLimits::known(&self.targets.balanced),
                ModelLimits::known(&self.targets.fast),
            ],
        };
        limits
            .into_iter()
            .map(|limits| limits.max_prompt_tokens)
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .min()
    }
}

fn resolve_copilot_model(model: &CopilotModel) -> ResolvedModel {
    ResolvedModel {
        model: model.id.clone(),
        capabilities: model.model_capabilities(),
    }
}

fn parse_reasoning_effort(value: &str) -> Option<ReasoningEffort> {
    match value {
        "none" => Some(ReasoningEffort::None),
        "minimal" => Some(ReasoningEffort::Minimal),
        "low" => Some(ReasoningEffort::Low),
        "medium" => Some(ReasoningEffort::Medium),
        "high" => Some(ReasoningEffort::High),
        "xhigh" => Some(ReasoningEffort::Xhigh),
        "max" => Some(ReasoningEffort::Max),
        _ => None,
    }
}

fn claude_tier(model: &str) -> Option<ModelTier> {
    let model = model.to_ascii_lowercase();
    if model.contains("haiku") {
        Some(ModelTier::Fast)
    } else if model.contains("sonnet") {
        Some(ModelTier::Balanced)
    } else if ["claude", "fable", "mythos", "opus"]
        .iter()
        .any(|family| model.contains(family))
    {
        Some(ModelTier::High)
    } else {
        None
    }
}

fn select_latest(models: &[CopilotModel], tier: ModelTier) -> Option<&CopilotModel> {
    let mut selected: Option<(&CopilotModel, Vec<u64>)> = None;
    for model in models {
        let Some(version) = version_for_tier(&model.id, tier) else {
            continue;
        };
        let replace = selected.as_ref().is_none_or(|(current, current_version)| {
            match compare_versions(&version, current_version) {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Equal => {
                    variant_rank(&model.id, tier) > variant_rank(&current.id, tier)
                }
                std::cmp::Ordering::Less => false,
            }
        });
        if replace {
            selected = Some((model, version));
        }
    }
    selected.map(|(model, _)| model)
}

fn variant_rank(id: &str, tier: ModelTier) -> (bool, String) {
    let id = id.to_ascii_lowercase();
    let stable = id
        .split_once('-')
        .is_some_and(|(_, rest)| rest.ends_with(&format!("-{}", tier.suffix())));
    (stable, id)
}

fn version_for_tier(id: &str, tier: ModelTier) -> Option<Vec<u64>> {
    let id = id.to_ascii_lowercase();
    let rest = id.strip_prefix("gpt-")?;
    let (version, suffix) = rest.split_once('-')?;
    if suffix != tier.suffix() && !suffix.starts_with(&format!("{}-", tier.suffix())) {
        return None;
    }
    version
        .split('.')
        .map(str::parse)
        .collect::<Result<Vec<_>, _>>()
        .ok()
        .filter(|version| !version.is_empty())
}

fn compare_versions(left: &[u64], right: &[u64]) -> std::cmp::Ordering {
    for index in 0..left.len().max(right.len()) {
        match left
            .get(index)
            .copied()
            .unwrap_or_default()
            .cmp(&right.get(index).copied().unwrap_or_default())
        {
            std::cmp::Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str) -> CopilotModel {
        CopilotModel {
            id: id.to_owned(),
            supported_endpoints: vec!["/responses".to_owned()],
            capabilities: CopilotCapabilities {
                supports: CopilotSupports {
                    parallel_tool_calls: Some(true),
                    reasoning_effort: vec![
                        "minimal".to_owned(),
                        "low".to_owned(),
                        "medium".to_owned(),
                        "future".to_owned(),
                        "max".to_owned(),
                    ],
                },
                limits: ModelLimits {
                    max_context_window_tokens: Some(1_050_000),
                    max_prompt_tokens: Some(922_000),
                    max_output_tokens: Some(128_000),
                },
            },
        }
    }

    fn catalog() -> Vec<CopilotModel> {
        vec![
            model("gpt-5.9-sol"),
            model("gpt-5.9-terra"),
            model("gpt-5.9-luna"),
            model("gpt-5.10-sol-preview"),
            model("gpt-5.10-terra"),
            model("gpt-5.10-luna"),
        ]
    }

    #[test]
    fn chooses_latest_numeric_version_for_each_tier() {
        let router = ModelRouter::copilot(catalog()).unwrap();

        assert_eq!(router.targets.high, "gpt-5.10-sol-preview");
        assert_eq!(router.targets.balanced, "gpt-5.10-terra");
        assert_eq!(router.targets.fast, "gpt-5.10-luna");
    }

    #[test]
    fn prefers_a_stable_alias_over_same_version_variants() {
        let mut models = catalog();
        models.push(model("gpt-5.10-sol"));

        let router = ModelRouter::copilot(models).unwrap();

        assert_eq!(router.targets.high, "gpt-5.10-sol");
    }

    #[test]
    fn filters_models_without_responses_support() {
        let mut models = catalog();
        let mut unavailable = model("gpt-99-sol");
        unavailable.supported_endpoints = vec!["/chat/completions".to_owned()];
        models.push(unavailable);

        let router = ModelRouter::copilot(models).unwrap();

        assert_eq!(router.targets.high, "gpt-5.10-sol-preview");
        assert!(router.resolve("gpt-99-sol").is_err());
    }

    #[test]
    fn maps_claude_families_to_tiers() {
        let router = ModelRouter::copilot(catalog()).unwrap();

        assert_eq!(
            router.resolve("claude-opus-5").unwrap().model,
            router.targets.high
        );
        assert_eq!(
            router.resolve("claude-future-6").unwrap().model,
            router.targets.high
        );
        assert_eq!(
            router.resolve("claude-sonnet-5").unwrap().model,
            router.targets.balanced
        );
        assert_eq!(
            router.resolve("claude-haiku-4-5").unwrap().model,
            router.targets.fast
        );
    }

    #[test]
    fn maps_unknown_non_gpt_models_to_high_tier() {
        let router = ModelRouter::copilot(catalog()).unwrap();

        assert_eq!(
            router.resolve("future-model").unwrap().model,
            router.targets.high
        );
    }

    #[test]
    fn resolves_exact_model_case_insensitively_with_capabilities() {
        let router = ModelRouter::copilot(catalog()).unwrap();

        let resolved = router.resolve("GPT-5.9-TERRA").unwrap();

        assert_eq!(resolved.model, "gpt-5.9-terra");
        assert!(resolved.capabilities.supports_parallel_tool_calls);
        assert_eq!(
            resolved.capabilities.reasoning_efforts,
            vec![
                ReasoningEffort::Minimal,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::Max
            ]
        );
        assert_eq!(
            resolved.capabilities.limits.max_prompt_tokens,
            Some(922_000)
        );
        assert_eq!(router.auto_compact_tokens(), Some(922_000));
    }

    #[test]
    fn rejects_unavailable_explicit_copilot_model() {
        let router = ModelRouter::copilot(catalog()).unwrap();

        assert_eq!(
            router.resolve("gpt-9-sol").unwrap_err().to_string(),
            "gpt-9-sol is not available from Copilot"
        );
    }

    #[test]
    fn requires_every_copilot_tier() {
        let error = ModelRouter::copilot([model("gpt-5.6-sol")]).unwrap_err();

        assert_eq!(
            error.to_string(),
            "Copilot has no Responses model in the terra tier"
        );
    }

    #[test]
    fn direct_router_maps_claude_and_passes_backend_models_through() {
        let targets = ModelTargets::default();
        let router = ModelRouter::direct(targets.clone());

        assert_eq!(router.resolve("opus").unwrap().model, targets.high);
        assert_eq!(router.resolve("sonnet").unwrap().model, targets.balanced);
        assert_eq!(router.resolve("haiku").unwrap().model, targets.fast);
        assert_eq!(router.resolve("o4-mini").unwrap().model, "o4-mini");
        assert_eq!(
            router.resolve("GPT-5.6-TERRA").unwrap().model,
            "gpt-5.6-terra"
        );
        assert!(
            !router
                .resolve("gpt-5.6-sol")
                .unwrap()
                .capabilities
                .reasoning_efforts
                .contains(&ReasoningEffort::Minimal)
        );
        assert_eq!(
            router.resolve("gpt-5.6-sol").unwrap().capabilities.limits,
            ModelLimits {
                max_context_window_tokens: Some(1_050_000),
                max_prompt_tokens: Some(922_000),
                max_output_tokens: Some(128_000),
            }
        );
        assert_eq!(router.auto_compact_tokens(), Some(922_000));
    }

    #[test]
    fn unknown_direct_models_do_not_get_guessed_limits() {
        let router = ModelRouter::direct(ModelTargets {
            high: "custom-high".into(),
            balanced: "custom-balanced".into(),
            fast: "custom-fast".into(),
        });

        assert_eq!(router.auto_compact_tokens(), None);
        assert_eq!(
            router.resolve("custom-high").unwrap().capabilities.limits,
            ModelLimits::default()
        );
    }

    #[test]
    fn deserializes_copilot_model_limits() {
        let model: CopilotModel = serde_json::from_value(serde_json::json!({
            "id": "gpt-5.6-sol",
            "supported_endpoints": ["/responses"],
            "capabilities": {
                "limits": {
                    "max_context_window_tokens": 1050000,
                    "max_prompt_tokens": 922000,
                    "max_output_tokens": 128000
                }
            }
        }))
        .unwrap();

        assert_eq!(
            model.capabilities.limits,
            ModelLimits {
                max_context_window_tokens: Some(1_050_000),
                max_prompt_tokens: Some(922_000),
                max_output_tokens: Some(128_000),
            }
        );
    }

    #[test]
    fn auto_compact_uses_the_lowest_selected_prompt_limit() {
        let mut models = catalog();
        models
            .iter_mut()
            .find(|model| model.id == "gpt-5.10-terra")
            .unwrap()
            .capabilities
            .limits
            .max_prompt_tokens = Some(900_000);

        let router = ModelRouter::copilot(models).unwrap();

        assert_eq!(router.auto_compact_tokens(), Some(900_000));
    }

    #[test]
    fn known_limits_fill_missing_copilot_catalog_fields() {
        let models = ["sol", "terra", "luna"].map(|tier| {
            let mut model = model(&format!("gpt-5.6-{tier}"));
            model.capabilities.limits = ModelLimits::default();
            model
        });

        let router = ModelRouter::copilot(models).unwrap();

        assert_eq!(router.auto_compact_tokens(), Some(922_000));
        assert_eq!(
            router
                .resolve("gpt-5.6-sol")
                .unwrap()
                .capabilities
                .limits
                .max_output_tokens,
            Some(128_000)
        );
    }
}
