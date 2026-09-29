use crate::types::*;
use csv::ReaderBuilder;
use std::collections::HashMap;


// ---------------------------------------------------------------------------
// Embedded CSV data (compile-time inclusion)
// ---------------------------------------------------------------------------

macro_rules! edg_csv {
    ($year:literal, $file:literal) => {
        include_str!(concat!(
            "../EDG_inputs/",
            $year,
            "/",
            $file
        ))
    };
}

const RESOURCES_2030: &str = edg_csv!("2030", "Resources_data.csv");
const FUELS_2030: &str = edg_csv!("2030", "Fuels_data.csv");
const LOAD_2030: &str = edg_csv!("2030", "Load_data.csv");
const VARIABILITY_2030: &str = edg_csv!("2030", "Resources_variability.csv");

const RESOURCES_2035: &str = edg_csv!("2035", "Resources_data.csv");
const FUELS_2035: &str = edg_csv!("2035", "Fuels_data.csv");
const LOAD_2035: &str = edg_csv!("2035", "Load_data.csv");
const VARIABILITY_2035: &str = edg_csv!("2035", "Resources_variability.csv");

const RESOURCES_2040: &str = edg_csv!("2040", "Resources_data.csv");
const FUELS_2040: &str = edg_csv!("2040", "Fuels_data.csv");
const LOAD_2040: &str = edg_csv!("2040", "Load_data.csv");
const VARIABILITY_2040: &str = edg_csv!("2040", "Resources_variability.csv");

const RESOURCES_2045: &str = edg_csv!("2045", "Resources_data.csv");
const FUELS_2045: &str = edg_csv!("2045", "Fuels_data.csv");
const LOAD_2045: &str = edg_csv!("2045", "Load_data.csv");
const VARIABILITY_2045: &str = edg_csv!("2045", "Resources_variability.csv");

const RESOURCES_2050: &str = edg_csv!("2050", "Resources_data.csv");
const FUELS_2050: &str = edg_csv!("2050", "Fuels_data.csv");
const LOAD_2050: &str = edg_csv!("2050", "Load_data.csv");
const VARIABILITY_2050: &str = edg_csv!("2050", "Resources_variability.csv");

struct YearCsvs {
    resources: &'static str,
    fuels: &'static str,
    load: &'static str,
    variability: &'static str,
}

fn year_csvs(year: u32) -> YearCsvs {
    match year {
        2030 => YearCsvs {
            resources: RESOURCES_2030,
            fuels: FUELS_2030,
            load: LOAD_2030,
            variability: VARIABILITY_2030,
        },
        2035 => YearCsvs {
            resources: RESOURCES_2035,
            fuels: FUELS_2035,
            load: LOAD_2035,
            variability: VARIABILITY_2035,
        },
        2040 => YearCsvs {
            resources: RESOURCES_2040,
            fuels: FUELS_2040,
            load: LOAD_2040,
            variability: VARIABILITY_2040,
        },
        2045 => YearCsvs {
            resources: RESOURCES_2045,
            fuels: FUELS_2045,
            load: LOAD_2045,
            variability: VARIABILITY_2045,
        },
        2050 => YearCsvs {
            resources: RESOURCES_2050,
            fuels: FUELS_2050,
            load: LOAD_2050,
            variability: VARIABILITY_2050,
        },
        _ => panic!("Invalid planning year: {}", year),
    }
}

// ---------------------------------------------------------------------------
// Embedded YAML game setup files
// ---------------------------------------------------------------------------

pub const SETUP_US: &str = include_str!("../game_setup/US_setup.yml");
pub const SETUP_CA: &str = include_str!("../game_setup/CA_setup.yml");
pub const SETUP_IL: &str = include_str!("../game_setup/IL_setup.yml");
pub const SETUP_NJ: &str = include_str!("../game_setup/NJ_setup.yml");
pub const SETUP_WY: &str = include_str!("../game_setup/WY_setup.yml");
pub const SETUP_US1: &str = include_str!("../game_setup/US_setup_1.yml");
pub const SETUP_US2: &str = include_str!("../game_setup/US_setup_2.yml");

pub fn builtin_setups() -> Vec<(&'static str, &'static str)> {
    vec![
        ("US_setup.yml", SETUP_US),
        ("CA_setup.yml", SETUP_CA),
        ("IL_setup.yml", SETUP_IL),
        ("NJ_setup.yml", SETUP_NJ),
        ("WY_setup.yml", SETUP_WY),
        ("US_setup_1.yml", SETUP_US1),
        ("US_setup_2.yml", SETUP_US2),
    ]
}

// ---------------------------------------------------------------------------
// CSV parsers
// ---------------------------------------------------------------------------

fn parse_fuels(csv_str: &str) -> HashMap<String, FuelRow> {
    let mut rdr = ReaderBuilder::new().from_reader(csv_str.as_bytes());
    let headers = rdr.headers().unwrap().clone();
    let idx = |name: &str| headers.iter().position(|h| h == name).unwrap();

    let fi_fuel = idx("Fuel");
    let fi_cost = idx("Cost");
    let fi_emissions = idx("Emissions");

    let mut map = HashMap::new();
    for result in rdr.records() {
        let rec = result.unwrap();
        let fuel = rec[fi_fuel].trim().to_string();
        let cost: f64 = rec[fi_cost].trim().parse().unwrap_or(0.0);
        let emissions: f64 = rec[fi_emissions].trim().parse().unwrap_or(0.0);
        map.insert(fuel.clone(), FuelRow { fuel, cost, emissions });
    }
    map
}

/// Parse Resources_data.csv and compute derived cost fields using fuel data.
fn parse_resources(csv_str: &str, fuels: &HashMap<String, FuelRow>) -> Vec<ResourceCsvRow> {
    let mut rdr = ReaderBuilder::new().from_reader(csv_str.as_bytes());
    let headers = rdr.headers().unwrap().clone();
    let idx = |name: &str| headers.iter().position(|h| h == name).unwrap();

    let fi_resource = idx("Resource");
    let fi_therm = idx("THERM");
    let fi_stor = idx("STOR");
    let fi_vre = idx("VRE");
    let fi_new_build = idx("New_Build");
    let fi_existing_cap_mw = idx("Existing_Cap_MW");
    let fi_existing_cap_mwh = idx("Existing_Cap_MWh");
    let fi_var_om = idx("Var_OM_Cost_per_MWh");
    let fi_heat_rate = idx("Heat_Rate_MMBTU_per_MWh");
    let fi_fuel = idx("Fuel");
    let fi_min_power = idx("Min_Power");
    let fi_ramp_up = idx("Ramp_Up_Percentage");
    let fi_ramp_dn = idx("Ramp_Dn_Percentage");
    let fi_eff_up = idx("Eff_Up");
    let fi_eff_down = idx("Eff_Down");

    let mut resources = Vec::new();
    for result in rdr.records() {
        let rec = result.unwrap();
        let parse_f64 = |i: usize| rec[i].trim().parse::<f64>().unwrap_or(0.0);
        let parse_i32 = |i: usize| rec[i].trim().parse::<i32>().unwrap_or(0);

        let fuel_name = rec[fi_fuel].trim().to_string();
        let fuel = fuels.get(&fuel_name).cloned().unwrap_or(FuelRow {
            fuel: fuel_name.clone(),
            cost: 0.0,
            emissions: 0.0,
        });

        let var_om = parse_f64(fi_var_om);
        let heat_rate = parse_f64(fi_heat_rate);
        let var_cost = var_om + fuel.cost * heat_rate;

        resources.push(ResourceCsvRow {
            resource: rec[fi_resource].trim().to_string(),
            therm: parse_i32(fi_therm),
            stor: parse_i32(fi_stor),
            vre: parse_i32(fi_vre),
            new_build: parse_i32(fi_new_build),
            existing_cap_mw: parse_f64(fi_existing_cap_mw),
            existing_cap_mwh: parse_f64(fi_existing_cap_mwh),
            var_om_cost_per_mwh: var_om,
            heat_rate_mmbtu_per_mwh: heat_rate,
            fuel: fuel_name,
            min_power: parse_f64(fi_min_power),
            ramp_up_percentage: parse_f64(fi_ramp_up),
            ramp_dn_percentage: parse_f64(fi_ramp_dn),
            eff_up: parse_f64(fi_eff_up),
            eff_down: parse_f64(fi_eff_down),
            var_cost,
        });
    }
    resources
}

/// Intermediate struct for parsed demand inputs.
pub struct DemandParsed {
    pub nse_segments: Vec<NseSegment>,
    pub hours_per_period: usize,
    pub sub_weights: Vec<f64>,
    pub time_index: Vec<usize>,
    pub load_mw_z1: Vec<f64>,
    pub sample_weight: Vec<f64>,
}

/// Parse Load_data.csv.
///
/// The file is "jagged": metadata columns (Voll, Demand_Segment, etc.) only
/// have values in the first few rows; Sub_Weights repeats once per rep period;
/// Time_Index and Load_MW_z1 repeat for every time step.
fn parse_demand(csv_str: &str) -> DemandParsed {
    // ---- collect all raw rows ----
    let mut rdr = ReaderBuilder::new()
        .flexible(true)
        .from_reader(csv_str.as_bytes());
    let headers = rdr.headers().unwrap().clone();
    let idx = |name: &str| headers.iter().position(|h| h == name);

    let fi_voll = idx("Voll").unwrap();
    let fi_seg = idx("Demand_Segment").unwrap();
    let fi_curtail_cost = idx("Cost_of_Demand_Curtailment_per_MW").unwrap();
    let fi_curtail_max = idx("Max_Demand_Curtailment").unwrap();
    let fi_rep_periods = idx("Rep_Periods").unwrap();
    let fi_timesteps = idx("Timesteps_per_Rep_Period").unwrap();
    let fi_sub_weights = idx("Sub_Weights").unwrap();
    let fi_time_index = idx("Time_Index").unwrap();
    let fi_load = idx("Load_MW_z1").unwrap();

    let get_opt = |rec: &csv::StringRecord, i: usize| -> Option<String> {
        rec.get(i).and_then(|s| {
            let t = s.trim().to_string();
            if t.is_empty() { None } else { Some(t) }
        })
    };

    let mut voll: f64 = 9000.0;
    let mut rep_periods: usize = 52;
    let mut hours_per_period: usize = 168;
    let mut demand_segments: Vec<usize> = Vec::new();
    let mut curtail_costs: Vec<f64> = Vec::new();
    let mut curtail_maxes: Vec<f64> = Vec::new();
    let mut sub_weights: Vec<f64> = Vec::new();
    let mut time_index: Vec<usize> = Vec::new();
    let mut load_mw_z1: Vec<f64> = Vec::new();

    for result in rdr.records() {
        let rec = result.unwrap();

        if let Some(v) = get_opt(&rec, fi_voll) {
            voll = v.parse().unwrap_or(voll);
        }
        if let Some(v) = get_opt(&rec, fi_rep_periods) {
            rep_periods = v.parse().unwrap_or(rep_periods);
        }
        if let Some(v) = get_opt(&rec, fi_timesteps) {
            hours_per_period = v.parse().unwrap_or(hours_per_period);
        }
        if let Some(v) = get_opt(&rec, fi_seg) {
            if let Ok(s) = v.parse::<usize>() {
                demand_segments.push(s);
            }
        }
        if let Some(v) = get_opt(&rec, fi_curtail_cost) {
            if let Ok(c) = v.parse::<f64>() {
                curtail_costs.push(c);
            }
        }
        if let Some(v) = get_opt(&rec, fi_curtail_max) {
            if let Ok(m) = v.parse::<f64>() {
                curtail_maxes.push(m);
            }
        }
        if let Some(v) = get_opt(&rec, fi_sub_weights) {
            if let Ok(w) = v.parse::<f64>() {
                sub_weights.push(w);
            }
        }
        if let Some(v) = get_opt(&rec, fi_time_index) {
            if let Ok(t) = v.parse::<usize>() {
                time_index.push(t);
                let load: f64 = get_opt(&rec, fi_load)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0.0);
                load_mw_z1.push(load);
            }
        }
    }

    // Build NSE segments
    let nse_segments: Vec<NseSegment> = demand_segments
        .iter()
        .enumerate()
        .map(|(i, &seg)| NseSegment {
            segment: seg,
            nse_cost: voll * curtail_costs[i],
            nse_max: curtail_maxes[i],
        })
        .collect();

    // Build sample weights: weight[t] = sub_weights[p] / hours_per_period
    // where p is the period index for time step t.
    let n_t = time_index.len();
    let mut sample_weight = vec![0.0f64; n_t];
    let mut t = 0;
    for p in 0..rep_periods {
        let w = if p < sub_weights.len() { sub_weights[p] } else { 1.0 };
        for _ in 0..hours_per_period {
            if t < n_t {
                sample_weight[t] = w / hours_per_period as f64;
                t += 1;
            }
        }
    }

    DemandParsed {
        nse_segments,
        hours_per_period,
        sub_weights,
        time_index,
        load_mw_z1,
        sample_weight,
    }
}

/// Parse Resources_variability.csv into a flat Vec indexed [t * N_RESOURCES + g].
/// The column order in the CSV must match RESOURCE_ORDER.
fn parse_variability(csv_str: &str, n_t: usize) -> Vec<f64> {
    let mut rdr = ReaderBuilder::new().from_reader(csv_str.as_bytes());
    let headers = rdr.headers().unwrap().clone();

    // Build mapping: resource index -> column index in CSV
    let col_for_resource: Vec<usize> = RESOURCE_ORDER
        .iter()
        .map(|name| {
            headers
                .iter()
                .position(|h| h == *name)
                .unwrap_or_else(|| panic!("Column '{}' not found in variability CSV", name))
        })
        .collect();

    let mut variability = vec![0.0f64; n_t * N_RESOURCES];
    let mut row = 0usize;
    for result in rdr.records() {
        if row >= n_t {
            break;
        }
        let rec = result.unwrap();
        for (g, &col) in col_for_resource.iter().enumerate() {
            variability[row * N_RESOURCES + g] =
                rec[col].trim().parse::<f64>().unwrap_or(0.0);
        }
        row += 1;
    }
    variability
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Load and assemble all simulation inputs for a given planning year,
/// applying the current `resource_params` (capacity overrides).
///
/// Mirrors `load_resources_input` + `load_demand_input` + `load_variability_input`
/// from EDG_engine.jl, plus the capacity-update logic in `run_stage`.
pub fn load_sim_inputs(
    year: u32,
    resource_params: &ResourceParams,
    is_new_nuclear: bool,
) -> SimInputs {
    let csvs = year_csvs(year);
    let fuels = parse_fuels(csvs.fuels);
    let mut csv_resources = parse_resources(csvs.resources, &fuels);
    let demand = parse_demand(csvs.load);
    let n_t = demand.time_index.len();
    let variability = parse_variability(csvs.variability, n_t);

    // --- Apply capacity overrides from resource_params ---
    // resource_params are indexed by *game block order* via names[].
    // Build a lookup: edg_data_name -> (start_capacity, build_cost, build_tokens).
    let params_by_name: HashMap<&str, (f64, f64, i32)> = resource_params
        .names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            (
                name.as_str(),
                (
                    resource_params.start_capacity[i],
                    resource_params.build_cost[i],
                    resource_params.build_tokens[i],
                ),
            )
        })
        .collect();

    for res in csv_resources.iter_mut() {
        if let Some(&(start_cap, build_cost, build_tokens)) = params_by_name.get(res.resource.as_str()) {
            // New capacity = (start_capacity + build_cost * build_tokens) * 1000 (GW -> MW)
            res.existing_cap_mw = (start_cap + build_cost * build_tokens as f64) * 1000.0;
        }
    }

    // Battery energy capacity = 4 × power capacity
    for res in csv_resources.iter_mut() {
        if res.resource == "battery" {
            res.existing_cap_mwh = res.existing_cap_mw * 4.0;
        }
    }

    // Retire existing capacity if not maintained:
    // For existing resources (New_Build == 0), cap is limited to
    // min(csv_cap, build_cost * build_tokens * 1000).
    for res in csv_resources.iter_mut() {
        if res.new_build == 0 {
            if res.resource == "nuclear" && is_new_nuclear {
                // New nuclear is maintained regardless of tokens
                continue;
            }
            if let Some(&(_, build_cost, build_tokens)) =
                params_by_name.get(res.resource.as_str())
            {
                let retained = build_cost * build_tokens as f64 * 1000.0;
                res.existing_cap_mw = res.existing_cap_mw.min(retained);
            }
        }
    }

    // --- Build Resource vec in canonical RESOURCE_ORDER ---
    // csv_resources rows are already in CSV order == RESOURCE_ORDER.
    let resources: Vec<Resource> = csv_resources
        .iter()
        .enumerate()
        .map(|(i, r)| Resource {
            id: i,
            name: r.resource.clone(),
            therm: r.therm,
            stor: r.stor,
            vre: r.vre,
            new_build: r.new_build,
            existing_cap_mw: r.existing_cap_mw,
            existing_cap_mwh: r.existing_cap_mwh,
            var_cost: r.var_cost,
            min_power: r.min_power,
            ramp_up_pct: r.ramp_up_percentage,
            ramp_dn_pct: r.ramp_dn_percentage,
            eff_up: r.eff_up,
            eff_down: r.eff_down,
        })
        .collect();

    SimInputs {
        resources,
        demand: demand.load_mw_z1,
        variability,
        sample_weight: demand.sample_weight,
        hours_per_period: demand.hours_per_period,
        nse_segments: demand.nse_segments,
    }
}

// ---------------------------------------------------------------------------
// Game setup YAML parsing
// ---------------------------------------------------------------------------

/// Raw serde structs mirroring the YAML schema — kept private to this module.
mod yaml_schema {
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct RawSetup {
        pub current_stage: Option<usize>,
        pub available_budget_tokens: i32,
        pub available_shaping_tokens: i32,
        pub current_stage_shaping_tokens: i32,
        pub available_build_tokens: Vec<i32>,
        pub stages: Vec<u32>,
        pub resource_blocks: std::collections::BTreeMap<String, RawBlock>,
        pub uncertainty_parameters: RawUncertainty,
        pub experience_rate: f64,
        pub backlash_rates: RawBacklashRates,
        pub scoring_parameters: RawScoring,
        pub shaping_tokens: RawShapingTokens,
        pub reliability_scores: Option<std::collections::BTreeMap<usize, i32>>,
        pub clean_scores: Option<std::collections::BTreeMap<usize, i32>>,
        #[serde(alias = "is_WY_setup")]
        pub is_wy_setup: Option<bool>,
    }

    #[derive(Deserialize)]
    pub struct RawBlock {
        pub name: String,
        pub start_capacity: f64,
        pub build_cost: f64,
        pub backlash_risk: String,
        #[serde(rename = "EDG_data_name")]
        pub edg_data_name: String,
        #[serde(rename = "EDG_data_info")]
        pub edg_data_info: String,
        pub new_resource: bool,
        pub social_backlash: bool,
        // per-stage built capacities (optional, present in saved files)
        pub cap_built_stage_1: Option<f64>,
        pub cap_built_stage_2: Option<f64>,
        pub cap_built_stage_3: Option<f64>,
        pub cap_built_stage_4: Option<f64>,
        pub cap_built_stage_5: Option<f64>,
    }

    #[derive(Deserialize)]
    pub struct RawUncertainty {
        #[serde(rename = "Demand_Variance")]
        pub demand_variance: f64,
        #[serde(rename = "Outage_Probability")]
        pub outage_probability: f64,
        #[serde(rename = "Outage_Rate")]
        pub outage_rate: f64,
        #[serde(rename = "Disaster_Probability")]
        pub disaster_probability: Vec<f64>,
    }

    #[derive(Deserialize)]
    pub struct RawBacklashRates {
        pub none: f64,
        pub low: f64,
        pub moderate: f64,
        pub high: f64,
    }

    #[derive(Deserialize)]
    pub struct RawScoring {
        #[serde(rename = "Max_Points")]
        pub max_points: usize,
        #[serde(rename = "Clean_Stage_1")]
        pub clean_stage_1: Vec<f64>,
        #[serde(rename = "Clean_Stage_2")]
        pub clean_stage_2: Vec<f64>,
        #[serde(rename = "Clean_Stage_3")]
        pub clean_stage_3: Vec<f64>,
        #[serde(rename = "Clean_Stage_4")]
        pub clean_stage_4: Vec<f64>,
        #[serde(rename = "Clean_Stage_5")]
        pub clean_stage_5: Vec<f64>,
        #[serde(rename = "Reliability")]
        pub reliability: Vec<f64>,
    }

    #[derive(Deserialize)]
    pub struct RawShapingTokens {
        pub resilience: bool,
        pub innovation_experience: bool,
        pub innovation_clean_firm: bool,
        pub social_license: bool,
    }
}

pub fn parse_game_setup(yaml_str: &str) -> GameSetup {
    use yaml_schema::*;
    let raw: RawSetup = serde_norway::from_str(yaml_str)
        .expect("Failed to parse game setup YAML");

    let current_stage = raw.current_stage.unwrap_or(1).max(1);

    // stages
    assert_eq!(raw.stages.len(), N_STAGES, "Expected 5 planning stages");
    let stages: [u32; N_STAGES] = raw.stages.try_into().unwrap();

    // available build tokens per stage
    let mut abt = [10i32; N_STAGES];
    for (i, v) in raw.available_build_tokens.iter().enumerate().take(N_STAGES) {
        abt[i] = *v;
    }

    // resource blocks (block_1 .. block_8, sorted by key)
    let mut block_keys: Vec<&String> = raw.resource_blocks.keys().collect();
    block_keys.sort();
    assert_eq!(block_keys.len(), N_RESOURCES, "Expected 8 resource blocks");

    let resource_blocks: Vec<ResourceBlock> = block_keys
        .iter()
        .map(|k| {
            let b = &raw.resource_blocks[*k];
            ResourceBlock {
                name: b.name.clone(),
                start_capacity: b.start_capacity,
                build_cost: b.build_cost,
                backlash_risk: BacklashLevel::from_str(&b.backlash_risk),
                edg_data_name: b.edg_data_name.clone(),
                edg_data_info: b.edg_data_info.clone(),
                new_resource: b.new_resource,
                social_backlash: b.social_backlash,
                cap_built: [
                    b.cap_built_stage_1.unwrap_or(0.0),
                    b.cap_built_stage_2.unwrap_or(0.0),
                    b.cap_built_stage_3.unwrap_or(0.0),
                    b.cap_built_stage_4.unwrap_or(0.0),
                    b.cap_built_stage_5.unwrap_or(0.0),
                ],
            }
        })
        .collect();
    let resource_blocks: [ResourceBlock; N_RESOURCES] = resource_blocks.try_into().unwrap();

    // uncertainty
    let dp: [f64; N_STAGES] = raw
        .uncertainty_parameters
        .disaster_probability
        .iter()
        .copied()
        .take(N_STAGES)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap_or([0.15, 0.3, 0.45, 0.6, 0.75]);

    let uncertainty_params = UncertaintyParams {
        demand_variance: raw.uncertainty_parameters.demand_variance,
        outage_probability: raw.uncertainty_parameters.outage_probability,
        outage_rate: raw.uncertainty_parameters.outage_rate,
        disaster_probability: dp,
    };

    let backlash_rates = BacklashRates {
        none: raw.backlash_rates.none,
        low: raw.backlash_rates.low,
        moderate: raw.backlash_rates.moderate,
        high: raw.backlash_rates.high,
    };

    // scoring (5 thresholds per stage, max 5 points)
    let clean_thresholds = [
        to_arr5(&raw.scoring_parameters.clean_stage_1),
        to_arr5(&raw.scoring_parameters.clean_stage_2),
        to_arr5(&raw.scoring_parameters.clean_stage_3),
        to_arr5(&raw.scoring_parameters.clean_stage_4),
        to_arr5(&raw.scoring_parameters.clean_stage_5),
    ];
    let reliability_thresholds = to_arr5(&raw.scoring_parameters.reliability);
    let scoring_params = ScoringParams {
        max_points: raw.scoring_parameters.max_points,
        clean_thresholds,
        reliability_thresholds,
    };

    // shaping tokens
    let shaping_tokens = ShapingTokensState {
        resilience: raw.shaping_tokens.resilience,
        innovation_experience: raw.shaping_tokens.innovation_experience,
        innovation_clean_firm: raw.shaping_tokens.innovation_clean_firm,
        social_license: raw.shaping_tokens.social_license,
    };

    // historical scores
    let to_score_arr = |opt: &Option<std::collections::BTreeMap<usize, i32>>| -> [i32; N_STAGES] {
        let mut arr = [0i32; N_STAGES];
        if let Some(m) = opt {
            for (&k, &v) in m {
                if k >= 1 && k <= N_STAGES {
                    arr[k - 1] = v;
                }
            }
        }
        arr
    };

    GameSetup {
        current_stage,
        available_budget_tokens: raw.available_budget_tokens,
        available_shaping_tokens: raw.available_shaping_tokens,
        current_stage_shaping_tokens: raw.current_stage_shaping_tokens,
        available_build_tokens: abt,
        stages,
        resource_blocks,
        uncertainty_params,
        experience_rate: raw.experience_rate,
        backlash_rates,
        scoring_params,
        shaping_tokens,
        reliability_scores: to_score_arr(&raw.reliability_scores),
        clean_scores: to_score_arr(&raw.clean_scores),
        is_wy_setup: raw.is_wy_setup.unwrap_or(false),
    }
}

fn to_arr5(v: &[f64]) -> [f64; 5] {
    let mut arr = [0.0f64; 5];
    for (i, &x) in v.iter().enumerate().take(5) {
        arr[i] = x;
    }
    arr
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn default_resource_params() -> ResourceParams {
        ResourceParams {
            names: RESOURCE_ORDER.map(|s| s.to_string()),
            start_capacity: [0.0; N_RESOURCES],
            build_cost: [1.0; N_RESOURCES],
            build_tokens: [0; N_RESOURCES],
        }
    }

    // -----------------------------------------------------------------------
    // parse_game_setup
    // -----------------------------------------------------------------------

    #[test]
    fn parse_game_setup_us_structure() {
        let setup = parse_game_setup(SETUP_US);
        assert_eq!(setup.stages.len(), N_STAGES);
        assert_eq!(setup.resource_blocks.len(), N_RESOURCES);
        assert_eq!(setup.available_build_tokens.len(), N_STAGES);
        assert!(setup.scoring_params.max_points > 0);
        assert!(setup.experience_rate >= 0.0);
    }

    #[test]
    fn parse_game_setup_all_builtins_succeed() {
        // Verify none of the built-in YAML files panic on parse
        for (name, yaml) in builtin_setups() {
            let setup = parse_game_setup(yaml);
            assert_eq!(
                setup.resource_blocks.len(), N_RESOURCES,
                "wrong resource count for {name}"
            );
            assert_eq!(setup.stages.len(), N_STAGES, "wrong stage count for {name}");
        }
    }

    #[test]
    fn only_wyoming_is_wy_setup() {
        for (name, yaml) in builtin_setups() {
            let setup = parse_game_setup(yaml);
            assert_eq!(setup.is_wy_setup, name == "WY_setup.yml", "{name}");
        }
    }

    #[test]
    fn parse_game_setup_stages_are_ascending() {
        let setup = parse_game_setup(SETUP_US);
        for i in 1..N_STAGES {
            assert!(
                setup.stages[i] > setup.stages[i - 1],
                "stages not ascending: {:?}", setup.stages
            );
        }
    }

    #[test]
    fn parse_game_setup_shaping_tokens_present() {
        // Shaping tokens should parse without panicking; default state is all false
        let setup = parse_game_setup(SETUP_US);
        // Just confirm the field exists and is accessible
        let _ = setup.shaping_tokens;
    }

    // -----------------------------------------------------------------------
    // load_sim_inputs
    // -----------------------------------------------------------------------

    #[test]
    fn load_sim_inputs_2030_structure() {
        let params = default_resource_params();
        let inputs = load_sim_inputs(2030, &params, false);
        assert_eq!(inputs.resources.len(), N_RESOURCES);
        assert!(!inputs.demand.is_empty(), "demand should be non-empty");
        assert_eq!(
            inputs.variability.len(),
            inputs.demand.len() * N_RESOURCES,
            "variability length mismatch"
        );
        assert_eq!(inputs.sample_weight.len(), inputs.demand.len());
        assert!(!inputs.nse_segments.is_empty(), "expected at least one NSE segment");
    }

    #[test]
    fn load_sim_inputs_all_years_parse() {
        let params = default_resource_params();
        for year in [2030u32, 2035, 2040, 2045, 2050] {
            let inputs = load_sim_inputs(year, &params, false);
            assert_eq!(inputs.resources.len(), N_RESOURCES, "year {year}");
            assert!(!inputs.demand.is_empty(), "year {year}: empty demand");
        }
    }

    #[test]
    fn load_sim_inputs_resources_in_canonical_order() {
        let params = default_resource_params();
        let inputs = load_sim_inputs(2030, &params, false);
        for (g, &expected_name) in RESOURCE_ORDER.iter().enumerate() {
            assert_eq!(
                inputs.resources[g].name, expected_name,
                "resource {g} name mismatch"
            );
        }
    }

    #[test]
    fn load_sim_inputs_build_tokens_increase_capacity() {
        // Adding build tokens should increase existing_cap_mw
        let mut params = default_resource_params();
        // Allocate 2 tokens to solar_pv (index 2), build_cost = 1.0 GW/token
        let solar_idx = resource_index("solar_pv").unwrap();
        params.build_tokens[solar_idx] = 2;
        params.build_cost[solar_idx] = 1.0; // 1 GW per token

        let inputs = load_sim_inputs(2030, &params, false);
        // start_capacity=0 + 1.0 * 2 = 2 GW = 2000 MW
        assert!(
            (inputs.resources[solar_idx].existing_cap_mw - 2000.0).abs() < 1.0,
            "solar cap = {} MW, expected 2000 MW",
            inputs.resources[solar_idx].existing_cap_mw
        );
    }

    #[test]
    fn load_sim_inputs_battery_mwh_is_4x_mw() {
        let mut params = default_resource_params();
        let batt_idx = resource_index("battery").unwrap();
        params.build_tokens[batt_idx] = 1;
        params.build_cost[batt_idx] = 1.0; // 1 GW/token → 1000 MW

        let inputs = load_sim_inputs(2030, &params, false);
        let batt = &inputs.resources[batt_idx];
        assert!(
            (batt.existing_cap_mwh - batt.existing_cap_mw * 4.0).abs() < 1.0,
            "battery MWh ({}) != 4 × MW ({})", batt.existing_cap_mwh, batt.existing_cap_mw
        );
    }
}
