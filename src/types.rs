/// Number of resource blocks (fixed per game design).
pub const N_RESOURCES: usize = 8;
/// Number of game stages.
pub const N_STAGES: usize = 5;

/// Canonical resource order matching the CSV files.
/// All internal arrays indexed by resource use this order.
pub const RESOURCE_ORDER: [&str; N_RESOURCES] = [
    "natural_gas",
    "nuclear",
    "solar_pv",
    "distributed_solar",
    "onshore_wind",
    "offshore_wind",
    "battery",
    "clean_firm",
];

pub fn resource_index(name: &str) -> Option<usize> {
    RESOURCE_ORDER.iter().position(|&n| n == name)
}

/// Display labels matching `RESOURCE_ORDER`.
pub const RESOURCE_LABELS: [&str; N_RESOURCES] = [
    "Natural Gas",
    "Nuclear",
    "Solar PV",
    "Dist. Solar",
    "Onshore Wind",
    "Offshore Wind",
    "Battery",
    "Clean Firm",
];

/// Display label for an EDG resource name (e.g. "solar_pv" -> "Solar PV").
pub fn resource_label(edg_name: &str) -> &str {
    resource_index(edg_name).map_or(edg_name, |g| RESOURCE_LABELS[g])
}

// ---------------------------------------------------------------------------
// Game setup (loaded from YAML at startup)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum BacklashLevel {
    None,
    Low,
    Moderate,
    High,
}

impl BacklashLevel {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "low" => Self::Low,
            "moderate" => Self::Moderate,
            "high" => Self::High,
            _ => Self::None,
        }
    }
}

/// Configuration for one resource block, loaded from the YAML setup file.
#[derive(Debug, Clone)]
pub struct ResourceBlock {
    pub name: String,
    /// Starting capacity in GW.
    pub start_capacity: f64,
    /// GW of capacity added per build token.
    pub build_cost: f64,
    pub backlash_risk: BacklashLevel,
    /// Name used in the EDG CSV data (e.g. "solar_pv").
    pub edg_data_name: String,
    /// Human-readable info string shown in the UI.
    pub edg_data_info: String,
    /// True = new buildable resource; false = existing (can only retain).
    pub new_resource: bool,
    /// Set to true after a social backlash event locks this resource.
    pub social_backlash: bool,
    // Per-stage built capacity (GW), filled in as the game progresses.
    pub cap_built: [f64; N_STAGES],
}

#[derive(Debug, Clone)]
pub struct BacklashRates {
    pub none: f64,
    pub low: f64,
    pub moderate: f64,
    pub high: f64,
}

impl BacklashRates {
    pub fn rate_for(&self, level: &BacklashLevel) -> f64 {
        match level {
            BacklashLevel::None => self.none,
            BacklashLevel::Low => self.low,
            BacklashLevel::Moderate => self.moderate,
            BacklashLevel::High => self.high,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UncertaintyParams {
    pub demand_variance: f64,
    pub outage_probability: f64,
    pub outage_rate: f64,
    /// One probability per stage.
    pub disaster_probability: [f64; N_STAGES],
}

/// Scoring thresholds.  `clean_thresholds[stage][point_level]` is the
/// minimum clean-energy share (%) needed to earn (max_points - point_level)
/// points in that stage.
#[derive(Debug, Clone)]
pub struct ScoringParams {
    pub max_points: usize,
    /// `[stage 0..4][threshold 0..max_points]`, descending order.
    pub clean_thresholds: [[f64; 5]; N_STAGES],
    /// Descending order, same for every stage.
    pub reliability_thresholds: [f64; 5],
}

/// Values of the shaping-token toggles, persisted across stages.
#[derive(Debug, Clone, Default)]
pub struct ShapingTokensState {
    pub resilience: bool,
    pub innovation_experience: bool,
    pub innovation_clean_firm: bool,
    pub social_license: bool,
}

/// Full game setup loaded from a YAML file.
#[derive(Debug, Clone)]
pub struct GameSetup {
    pub current_stage: usize, // 1-based
    pub available_budget_tokens: i32,
    pub available_shaping_tokens: i32,
    pub current_stage_shaping_tokens: i32,
    /// Build-token budget per stage.
    pub available_build_tokens: [i32; N_STAGES],
    /// Planning years, e.g. [2030, 2035, 2040, 2045, 2050].
    pub stages: [u32; N_STAGES],
    pub resource_blocks: [ResourceBlock; N_RESOURCES],
    pub uncertainty_params: UncertaintyParams,
    pub experience_rate: f64,
    pub backlash_rates: BacklashRates,
    pub scoring_params: ScoringParams,
    pub shaping_tokens: ShapingTokensState,
    pub reliability_scores: [i32; N_STAGES],
    pub clean_scores: [i32; N_STAGES],
    pub is_wy_setup: bool,
}

// ---------------------------------------------------------------------------
// Resource parameters (mutable during a game session)
// ---------------------------------------------------------------------------

/// The three mutable per-resource parameters that evolve across stages.
///
/// All arrays are in *block order* (the order of `resource_blocks` in the
/// setup YAML), which generally differs from `RESOURCE_ORDER`.
#[derive(Debug, Clone)]
pub struct ResourceParams {
    /// Internal EDG data names, e.g. "solar_pv".  Fixed for the session.
    pub names: [String; N_RESOURCES],
    /// Starting capacity heading into the *current* stage (GW).
    pub start_capacity: [f64; N_RESOURCES],
    /// Build cost (GW per token), decreases via experience curves.
    pub build_cost: [f64; N_RESOURCES],
    /// Tokens allocated for the current stage (reset to 0 after advancing).
    pub build_tokens: [i32; N_RESOURCES],
}

/// Integer shaping-token counts passed to the solver/engine.
#[derive(Debug, Clone, Default)]
pub struct ShapingTokens {
    pub resilience: i32,
    pub innovation_experience: i32,
    pub innovation_clean_firm: i32,
    pub social_license: i32,
}

// ---------------------------------------------------------------------------
// CSV data (parsed from embedded files at startup)
// ---------------------------------------------------------------------------

/// One row from Resources_data.csv (only the fields used by the solver).
#[derive(Debug, Clone)]
pub struct ResourceCsvRow {
    pub resource: String,
    pub therm: i32,
    pub stor: i32,
    pub vre: i32,
    pub new_build: i32,
    pub existing_cap_mw: f64,
    pub existing_cap_mwh: f64,
    pub var_om_cost_per_mwh: f64,
    pub heat_rate_mmbtu_per_mwh: f64,
    pub fuel: String,
    pub min_power: f64,
    pub ramp_up_percentage: f64,
    pub ramp_dn_percentage: f64,
    pub eff_up: f64,
    pub eff_down: f64,
    /// Pre-computed total variable cost ($/MWh) = var_om + fuel_cost * heat_rate.
    pub var_cost: f64,
}

/// One row from Fuels_data.csv.
#[derive(Debug, Clone)]
pub struct FuelRow {
    pub fuel: String,
    pub cost: f64,
    pub emissions: f64,
}

/// One non-served energy segment from Load_data.csv.
#[derive(Debug, Clone)]
pub struct NseSegment {
    pub segment: usize, // 1-based
    /// Cost per MWh of curtailment.
    pub nse_cost: f64,
    /// Maximum fraction of demand that can be curtailed in this segment.
    pub nse_max: f64,
}

// ---------------------------------------------------------------------------
// Solver inputs (assembled from CSV data + current ResourceParams)
// ---------------------------------------------------------------------------

/// A fully-prepared resource record handed to the LP solver.
#[derive(Debug, Clone)]
pub struct Resource {
    pub id: usize, // 0-based index into the resource arrays
    pub name: String,
    pub therm: i32,
    pub stor: i32,
    pub vre: i32,
    pub new_build: i32,
    pub existing_cap_mw: f64,
    pub existing_cap_mwh: f64,
    /// Total variable cost ($/MWh), pre-computed from CSV + fuel data.
    pub var_cost: f64,
    pub min_power: f64,
    pub ramp_up_pct: f64,
    pub ramp_dn_pct: f64,
    pub eff_up: f64,
    pub eff_down: f64,
}

/// All inputs required to run the LP dispatch simulation.
#[derive(Debug, Clone)]
pub struct SimInputs {
    pub resources: Vec<Resource>,
    /// Hourly load in MW for each time step.
    pub demand: Vec<f64>,
    /// Capacity factor for each (time_step, resource) pair.
    /// Indexed as `variability[t * n_resources + g]`.
    pub variability: Vec<f64>,
    /// Weight of each time step (how many full-year hours it represents).
    pub sample_weight: Vec<f64>,
    pub hours_per_period: usize,
    pub nse_segments: Vec<NseSegment>,
}

// ---------------------------------------------------------------------------
// Simulation / stage results
// ---------------------------------------------------------------------------

/// Per-resource summary for one stage.
#[derive(Debug, Clone)]
pub struct ResourceResult {
    pub id: usize,
    pub resource: String,
    pub gwh: f64,
    pub percent_gwh: f64,
    pub ending_capacity_gw: f64,
    pub capacity_factor: f64,
}

/// Non-served energy and reliability summary for one stage.
#[derive(Debug, Clone)]
pub struct NseResult {
    pub max_nse_gw: f64,
    pub total_nse_gwh: f64,
    pub nse_percent_of_demand: f64,
    pub reliability: f64,
    pub reserve_margin: f64,
}

/// Hourly dispatch record for one time step (used for plotting).
#[derive(Debug, Clone)]
pub struct DispatchHour {
    pub hour: usize,
    pub demand_gw: f64,
    pub storage_level: f64,
    /// Negative = charging; positive = discharging (convention matches Julia).
    pub storage_charge: f64,
    pub nonserved: f64,
    /// Generation (GW) for each resource, in the same order as `resources`.
    pub generation: [f64; N_RESOURCES],
}

/// Demand/disaster uncertainty realisation for one stage.
#[derive(Debug, Clone)]
pub struct UncertaintyResult {
    /// Fractional demand shock, e.g. 0.03 = +3 %.
    pub demand_shock_percent: f64,
    pub disaster: bool,
    pub outage_rate: f64,
    /// Week number (1-based) of the disaster; -1 if no disaster.
    pub outage_week: i32,
    /// Indexed in `RESOURCE_ORDER` (CSV order).
    pub forced_outages: [bool; N_RESOURCES],
}

/// Reliability and clean-energy scores for one stage.
#[derive(Debug, Clone, Default)]
pub struct StageScores {
    pub reliability: f64,
    pub reliability_points: i32,
    pub clean_share: f64,
    pub clean_points: i32,
}

/// Everything returned by `run_simulation` (preview, no uncertainty).
#[derive(Debug, Clone)]
pub struct SimulationResults {
    pub scores: StageScores,
    pub dispatch: Vec<DispatchHour>,
    pub resource_results: Vec<ResourceResult>,
    pub nse_result: NseResult,
}

/// Everything returned by `advance_stage` (includes uncertainty events).
#[derive(Debug, Clone)]
pub struct StageResults {
    pub resource_results: Vec<ResourceResult>,
    pub nse_result: NseResult,
    pub dispatch: Vec<DispatchHour>,
    pub uncertainty: UncertaintyResult,
    pub scores: StageScores,
    /// Updated starting capacities (GW) for the next stage, in block order.
    pub next_start_capacity: [f64; N_RESOURCES],
    /// Updated build costs (GW/token) after experience curves, in block order.
    pub next_build_cost: [f64; N_RESOURCES],
}

/// Which resources experienced a social backlash event (block order).
#[derive(Debug, Clone, Default)]
pub struct SocialBacklash {
    pub backlash: [bool; N_RESOURCES],
}

/// Learning-curve cost reductions realised this stage (block order).
#[derive(Debug, Clone, Default)]
pub struct ExperienceResults {
    pub experience_rate: [f64; N_RESOURCES],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_index_all_known() {
        assert_eq!(resource_index("natural_gas"), Some(0));
        assert_eq!(resource_index("nuclear"), Some(1));
        assert_eq!(resource_index("solar_pv"), Some(2));
        assert_eq!(resource_index("distributed_solar"), Some(3));
        assert_eq!(resource_index("onshore_wind"), Some(4));
        assert_eq!(resource_index("offshore_wind"), Some(5));
        assert_eq!(resource_index("battery"), Some(6));
        assert_eq!(resource_index("clean_firm"), Some(7));
    }

    #[test]
    fn resource_index_unknown_returns_none() {
        assert_eq!(resource_index("coal"), None);
        assert_eq!(resource_index("hydro"), None);
        assert_eq!(resource_index(""), None);
        assert_eq!(resource_index("Solar_PV"), None); // case-sensitive
    }

    #[test]
    fn backlash_level_from_str_all_variants() {
        assert!(matches!(BacklashLevel::from_str("low"), BacklashLevel::Low));
        assert!(matches!(BacklashLevel::from_str("LOW"), BacklashLevel::Low));
        assert!(matches!(BacklashLevel::from_str("Low"), BacklashLevel::Low));
        assert!(matches!(BacklashLevel::from_str("moderate"), BacklashLevel::Moderate));
        assert!(matches!(BacklashLevel::from_str("MODERATE"), BacklashLevel::Moderate));
        assert!(matches!(BacklashLevel::from_str("high"), BacklashLevel::High));
        assert!(matches!(BacklashLevel::from_str("HIGH"), BacklashLevel::High));
        assert!(matches!(BacklashLevel::from_str("none"), BacklashLevel::None));
        assert!(matches!(BacklashLevel::from_str(""), BacklashLevel::None));
        assert!(matches!(BacklashLevel::from_str("unknown"), BacklashLevel::None));
    }

    #[test]
    fn backlash_rates_rate_for_all_levels() {
        let rates = BacklashRates { none: 0.0, low: 0.05, moderate: 0.15, high: 0.4 };
        assert_eq!(rates.rate_for(&BacklashLevel::None), 0.0);
        assert_eq!(rates.rate_for(&BacklashLevel::Low), 0.05);
        assert_eq!(rates.rate_for(&BacklashLevel::Moderate), 0.15);
        assert_eq!(rates.rate_for(&BacklashLevel::High), 0.4);
    }
}
