use rand::Rng;

use crate::data::load_sim_inputs;
use crate::solver::{self, SolverResult};
use crate::types::*;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Preview simulation: no uncertainty applied, returns summary for scoring display.
pub fn run_simulation(
    planning_year: u32,
    resource_params: &ResourceParams,
    scoring_params: &ScoringParams,
    shaping_tokens: &ShapingTokens,
    stage_num: usize,     // 1-based
    is_new_nuclear: bool,
) -> SimulationResults {
    let mut inputs = load_sim_inputs(planning_year, resource_params, is_new_nuclear);
    apply_clean_firm_gate(&mut inputs, shaping_tokens);

    let sol = solver::solve(&inputs);
    let (resource_results, nse_result, dispatch) = compute_results(&inputs, &sol);

    let clean_share = 100.0
        - resource_results
            .iter()
            .find(|r| r.resource == "natural_gas")
            .map(|r| r.percent_gwh)
            .unwrap_or(0.0);

    let scores = calc_scores(stage_num, nse_result.reliability, clean_share, scoring_params);

    SimulationResults {
        scores,
        dispatch,
        resource_results,
        nse_result,
    }
}

/// Full stage advance: applies uncertainty, solves, computes scores, and
/// returns updated capacity/cost parameters for the next stage.
pub fn advance_stage(
    stage_num: usize,     // 1-based
    planning_year: u32,
    resource_params: &ResourceParams,
    shaping_tokens: &ShapingTokens,
    uncertainty_params: &UncertaintyParams,
    scoring_params: &ScoringParams,
    experience_rate: f64,
    resource_blocks: &[ResourceBlock; N_RESOURCES],
    backlash_rates: &BacklashRates,
    is_wy_setup: bool,
    is_new_nuclear: bool,
) -> (StageResults, SocialBacklash, ExperienceResults) {
    let mut rng = rand::thread_rng();

    let mut inputs = load_sim_inputs(planning_year, resource_params, is_new_nuclear);
    apply_clean_firm_gate(&mut inputs, shaping_tokens);

    let uncertainty = resolve_uncertainty(
        &mut inputs,
        uncertainty_params,
        shaping_tokens,
        stage_num,
        &mut rng,
    );

    let sol = solver::solve(&inputs);
    let (resource_results, nse_result, dispatch) = compute_results(&inputs, &sol);

    let clean_share = 100.0
        - resource_results
            .iter()
            .find(|r| r.resource == "natural_gas")
            .map(|r| r.percent_gwh)
            .unwrap_or(0.0);

    let scores = calc_scores(stage_num, nse_result.reliability, clean_share, scoring_params);

    // Next-stage starting capacities: take the ending MW and convert to GW.
    let mut next_start_capacity = [0.0f64; N_RESOURCES];
    for g in 0..N_RESOURCES {
        next_start_capacity[g] = inputs.resources[g].existing_cap_mw / 1000.0;
    }

    let (social_backlash, mut next_build_cost, experience_results) = update_step(
        resource_params,
        shaping_tokens,
        experience_rate,
        resource_blocks,
        backlash_rates,
        is_new_nuclear,
        &mut rng,
    );

    // Special nuclear build-cost adjustments (game-design rule, non-WY setups only)
    if !is_wy_setup {
        let nuclear_g = resource_index("nuclear").unwrap_or(1);
        if planning_year == 2030 {
            // After 2030, nuclear becomes harder next stage (halve GW/token)
            next_build_cost[nuclear_g] = (next_build_cost[nuclear_g] / 2.0).ceil();
        } else if planning_year == 2035 {
            // After 2035, nuclear gets somewhat easier (double GW/token)
            next_build_cost[nuclear_g] *= 2.0;
        }
    }

    let stage_results = StageResults {
        resource_results,
        nse_result,
        dispatch,
        uncertainty,
        scores,
        next_start_capacity,
        next_build_cost,
    };

    (stage_results, social_backlash, experience_results)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Zero out clean-firm capacity unless the Innovation: Clean Firm shaping token is active.
fn apply_clean_firm_gate(inputs: &mut SimInputs, shaping_tokens: &ShapingTokens) {
    if shaping_tokens.innovation_clean_firm == 0 {
        if let Some(g) = resource_index("clean_firm") {
            inputs.resources[g].existing_cap_mw = 0.0;
        }
    }
}

/// Apply random demand shock and (optionally) a climate-disaster outage event.
fn resolve_uncertainty(
    inputs: &mut SimInputs,
    params: &UncertaintyParams,
    shaping_tokens: &ShapingTokens,
    stage_num: usize,  // 1-based
    rng: &mut impl Rng,
) -> UncertaintyResult {
    let n_t = inputs.demand.len();
    let n_g = inputs.resources.len();

    // Demand shock ~ N(0, demand_variance); scale every hour by (1 + shock).
    let shock = sample_normal(0.0, params.demand_variance, rng);
    let factor = 1.0 + shock;
    for d in &mut inputs.demand {
        *d = (*d * factor).round();
    }

    // Climate disaster
    let disaster_prob = params.disaster_probability[(stage_num - 1).min(N_STAGES - 1)];
    let disaster = rng.gen::<f64>() < disaster_prob;

    let mut forced_outages = [false; N_RESOURCES];
    let mut outage_week: i32 = -1;

    if disaster {
        // Draw a week number in [22, 38] (matching Julia: start=22, end=38)
        let week = (rng.gen::<f64>() * 17.0 + 22.0).floor() as usize;
        let week = week.min(38); // clamp to [22, 38]
        outage_week = (week + 1) as i32; // stored 1-indexed as in Julia

        // Affected time steps: 168 * week .. 168 * week + 672 (four weeks)
        let t_start = 168 * week;
        let t_end = (t_start + 672).min(n_t);

        let outage_prob = if shaping_tokens.resilience > 0 {
            params.outage_probability / 2.0
        } else {
            params.outage_probability
        };

        for g in 0..n_g {
            if rng.gen::<f64>() < outage_prob {
                forced_outages[g] = true;
                let reduction = 1.0 - params.outage_rate;
                for t in t_start..t_end {
                    inputs.variability[t * n_g + g] *= reduction;
                }
            }
        }
    }

    UncertaintyResult {
        demand_shock_percent: shock * 100.0,
        disaster,
        outage_rate: params.outage_rate,
        outage_week,
        forced_outages,
    }
}

/// Compute social backlash and experience-curve cost reductions for this stage.
///
/// Returns `(SocialBacklash, new_build_cost, ExperienceResults)`.
fn update_step(
    resource_params: &ResourceParams,
    shaping_tokens: &ShapingTokens,
    experience_rate: f64,
    resource_blocks: &[ResourceBlock; N_RESOURCES],
    backlash_rates: &BacklashRates,
    is_new_nuclear: bool,
    rng: &mut impl Rng,
) -> (SocialBacklash, [f64; N_RESOURCES], ExperienceResults) {
    let mut backlash = [false; N_RESOURCES];
    let mut new_build_cost = resource_params.build_cost;
    let mut experience = [0.0f64; N_RESOURCES];

    for g in 0..N_RESOURCES {
        let name = &resource_params.names[g];
        let build_tokens = resource_params.build_tokens[g];

        // Look up backlash risk for this resource by EDG name
        let risk = resource_blocks
            .iter()
            .find(|b| &b.edg_data_name == name)
            .map(|b| &b.backlash_risk)
            .unwrap_or(&BacklashLevel::None);

        let base_rate = backlash_rates.rate_for(risk);
        let backlash_rate = if shaping_tokens.social_license > 0 {
            base_rate / 2.0
        } else {
            base_rate
        };

        // One Bernoulli draw per build token; any success → backlash.
        for _ in 0..build_tokens.max(0) {
            if rng.gen::<f64>() < backlash_rate {
                backlash[g] = true;
                break;
            }
        }

        // Experience curve: only for new (non-existing) resources with tokens spent.
        let is_existing = name == "natural_gas"
            || (name == "nuclear" && !is_new_nuclear);

        if !is_existing && build_tokens > 0 {
            let (mean, std) = if shaping_tokens.innovation_experience > 0 {
                (experience_rate * 2.0, experience_rate)
            } else {
                (experience_rate, experience_rate / 2.0)
            };
            let exp_val = sample_normal(mean, std, rng).max(0.01);
            experience[g] = (exp_val * 1000.0).round() / 1000.0; // 3 decimal places

            // Build cost is GW/token; dividing by (1-exp)^tokens increases it
            // (each token builds more GW → better learning).
            let denom = (1.0 - exp_val).powi(build_tokens);
            if denom > 0.0 {
                new_build_cost[g] = (new_build_cost[g] / denom).round();
            }
        }
    }

    (
        SocialBacklash { backlash },
        new_build_cost,
        ExperienceResults { experience_rate: experience },
    )
}

/// Compute ResourceResult, NseResult, and DispatchHour from a solved LP.
fn compute_results(
    inputs: &SimInputs,
    sol: &SolverResult,
) -> (Vec<ResourceResult>, NseResult, Vec<DispatchHour>) {
    let n_t = inputs.demand.len();
    let n_g = inputs.resources.len();
    let n_s = inputs.nse_segments.len();
    let n_stor = sol.stor_indices.len();

    // --- Weighted annual generation (MWh) per resource ---
    let mut gen_mwh = vec![0.0f64; n_g];
    for t in 0..n_t {
        let w = inputs.sample_weight[t];
        for g in 0..n_g {
            gen_mwh[g] += w * sol.gen[t * n_g + g];
        }
    }
    // Storage: subtract charging (net generation = discharge - charge)
    for (stor_local, &stor_g) in sol.stor_indices.iter().enumerate() {
        for t in 0..n_t {
            gen_mwh[stor_g] -= inputs.sample_weight[t] * sol.charge[t * n_stor + stor_local];
        }
    }

    // Convert to GWh
    let gen_gwh: Vec<f64> = gen_mwh.iter().map(|&v| v / 1000.0).collect();

    // Total generation (non-storage only) for percentage calculations
    let stor_set: std::collections::HashSet<usize> =
        sol.stor_indices.iter().cloned().collect();
    let total_gen_gwh: f64 = gen_gwh
        .iter()
        .enumerate()
        .filter(|(g, _)| !stor_set.contains(g))
        .map(|(_, &v)| v)
        .sum();

    // Total annual demand (GWh)
    let total_demand_gwh: f64 = inputs
        .demand
        .iter()
        .zip(&inputs.sample_weight)
        .map(|(d, w)| w * d)
        .sum::<f64>()
        / 1000.0;

    // --- ResourceResult ---
    let resource_results: Vec<ResourceResult> = inputs
        .resources
        .iter()
        .enumerate()
        .map(|(g, res)| {
            let gwh = gen_gwh[g];
            let percent_gwh = if total_gen_gwh > 0.0 {
                gwh / total_gen_gwh * 100.0
            } else {
                0.0
            };
            let cap_gw = res.existing_cap_mw / 1000.0;
            let capacity_factor = if cap_gw > 0.0 {
                gwh / 8760.0 / cap_gw * 100.0
            } else {
                0.0
            };
            ResourceResult {
                id: g,
                resource: res.name.clone(),
                gwh,
                percent_gwh,
                ending_capacity_gw: cap_gw,
                capacity_factor,
            }
        })
        .collect();

    // --- NSE results ---
    let mut nse_gwh = vec![0.0f64; n_s];
    let mut max_nse_mw = vec![0.0f64; n_s];
    for t in 0..n_t {
        let w = inputs.sample_weight[t];
        for s in 0..n_s {
            let v = sol.nse[t * n_s + s];
            nse_gwh[s] += w * v / 1000.0;
            if v > max_nse_mw[s] {
                max_nse_mw[s] = v;
            }
        }
    }
    let total_nse_gwh: f64 = nse_gwh.iter().sum();
    let max_nse_gw = max_nse_mw.first().copied().unwrap_or(0.0) / 1000.0;
    let reliability = if total_demand_gwh > 0.0 {
        (1.0 - total_nse_gwh / total_demand_gwh) * 100.0
    } else {
        100.0
    };
    let nse_pct = if total_demand_gwh > 0.0 {
        total_nse_gwh / total_demand_gwh * 100.0
    } else {
        0.0
    };

    // Reserve margin: min over all hours of (firm + VRE + battery_net - demand)
    let firm_resources = ["nuclear", "natural_gas", "clean_firm"];
    let vre_resources = [
        "solar_pv",
        "distributed_solar",
        "onshore_wind",
        "offshore_wind",
    ];

    let firm_cap_gw: f64 = firm_resources
        .iter()
        .filter_map(|name| resource_index(name))
        .map(|g| inputs.resources[g].existing_cap_mw / 1000.0)
        .sum();

    let reserve_margin = (0..n_t)
        .map(|t| {
            let vre_gw: f64 = vre_resources
                .iter()
                .filter_map(|name| resource_index(name))
                .map(|g| {
                    inputs.resources[g].existing_cap_mw / 1000.0
                        * inputs.variability[t * n_g + g]
                })
                .sum();

            // Battery: dispatch (discharge) − battery_charge column (= −charge/1000).
            // Matches Julia: dispatch_results.battery − dispatch_results.battery_charge
            // where battery_charge = vCHARGE / (−1000).
            let batt_term: f64 = sol
                .stor_indices
                .iter()
                .enumerate()
                .map(|(sl, &sg)| {
                    let discharge_gw = sol.gen[t * n_g + sg] / 1000.0;
                    let charge_col = sol.charge[t * n_stor + sl] / (-1000.0); // negative when charging
                    discharge_gw - charge_col
                })
                .sum();

            firm_cap_gw + vre_gw + batt_term - inputs.demand[t] / 1000.0
        })
        .fold(f64::INFINITY, f64::min);

    let reserve_margin = if reserve_margin.is_infinite() {
        0.0
    } else {
        (reserve_margin * 10.0).round() / 10.0
    };

    let nse_result = NseResult {
        max_nse_gw: max_nse_gw.round(),
        total_nse_gwh: total_nse_gwh.round(),
        nse_percent_of_demand: (nse_pct * 100.0).round() / 100.0,
        reliability: (reliability * 100.0).round() / 100.0,
        reserve_margin,
    };

    // --- DispatchHour ---
    let batt_sl: Option<usize> = sol.stor_indices.iter().enumerate().find_map(|(sl, &sg)| {
        if inputs.resources[sg].name == "battery" {
            Some(sl)
        } else {
            None
        }
    });

    let dispatch: Vec<DispatchHour> = (0..n_t)
        .map(|t| {
            let storage_level = batt_sl
                .map(|sl| sol.soc[t * n_stor + sl] / 1000.0)
                .unwrap_or(0.0);
            let storage_charge = batt_sl
                .map(|sl| sol.charge[t * n_stor + sl] / (-1000.0)) // negative = charging
                .unwrap_or(0.0);
            let nonserved = if n_s > 0 { sol.nse[t * n_s] / 1000.0 } else { 0.0 };

            let mut generation = [0.0f64; N_RESOURCES];
            for g in 0..n_g.min(N_RESOURCES) {
                generation[g] = sol.gen[t * n_g + g] / 1000.0;
            }

            DispatchHour {
                hour: t,
                demand_gw: inputs.demand[t] / 1000.0,
                storage_level,
                storage_charge,
                nonserved,
                generation,
            }
        })
        .collect();

    (resource_results, nse_result, dispatch)
}

/// Compute reliability and clean-energy scores for a stage.
pub fn calc_scores(
    stage_num: usize,     // 1-based
    reliability: f64,
    clean_share: f64,
    scoring_params: &ScoringParams,
) -> StageScores {
    let max_pts = scoring_params.max_points as i32;
    let n = scoring_params.max_points.min(5);
    let stage_idx = (stage_num - 1).min(N_STAGES - 1);

    let mut reliability_points = max_pts;
    for &threshold in &scoring_params.reliability_thresholds[..n] {
        if reliability >= threshold {
            break;
        }
        reliability_points -= 1;
    }

    let mut clean_points = max_pts;
    for &threshold in &scoring_params.clean_thresholds[stage_idx][..n] {
        if clean_share >= threshold {
            break;
        }
        clean_points -= 1;
    }

    StageScores {
        reliability,
        reliability_points,
        clean_share,
        clean_points,
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

/// Box-Muller normal sample: N(mean, std).
fn sample_normal(mean: f64, std: f64, rng: &mut impl Rng) -> f64 {
    // u1 must be > 0 to avoid log(0); gen::<f64>() ∈ [0.0, 1.0)
    let u1 = (1.0_f64 - rng.gen::<f64>()).max(1e-15);
    let u2: f64 = rng.gen();
    let z = (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos();
    mean + std * z
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn test_scoring_params() -> ScoringParams {
        ScoringParams {
            max_points: 5,
            clean_thresholds: [
                [80.0, 60.0, 40.0, 20.0, 5.0],
                [85.0, 65.0, 45.0, 25.0, 10.0],
                [90.0, 70.0, 50.0, 30.0, 15.0],
                [92.0, 75.0, 55.0, 35.0, 20.0],
                [95.0, 80.0, 60.0, 40.0, 25.0],
            ],
            reliability_thresholds: [99.9, 99.5, 99.0, 98.0, 95.0],
        }
    }

    #[test]
    fn calc_scores_perfect() {
        let p = test_scoring_params();
        let s = calc_scores(1, 100.0, 100.0, &p);
        assert_eq!(s.reliability_points, 5);
        assert_eq!(s.clean_points, 5);
        assert_eq!(s.reliability, 100.0);
        assert_eq!(s.clean_share, 100.0);
    }

    #[test]
    fn calc_scores_zero_reliability_and_clean() {
        let p = test_scoring_params();
        let s = calc_scores(1, 0.0, 0.0, &p);
        assert_eq!(s.reliability_points, 0);
        assert_eq!(s.clean_points, 0);
    }

    #[test]
    fn calc_scores_reliability_thresholds() {
        let p = test_scoring_params();
        // reliability_thresholds = [99.9, 99.5, 99.0, 98.0, 95.0]
        assert_eq!(calc_scores(1, 99.9, 100.0, &p).reliability_points, 5);
        assert_eq!(calc_scores(1, 99.8, 100.0, &p).reliability_points, 4);
        assert_eq!(calc_scores(1, 99.2, 100.0, &p).reliability_points, 3);
        assert_eq!(calc_scores(1, 98.5, 100.0, &p).reliability_points, 2);
        assert_eq!(calc_scores(1, 96.0, 100.0, &p).reliability_points, 1);
        assert_eq!(calc_scores(1, 90.0, 100.0, &p).reliability_points, 0);
    }

    #[test]
    fn calc_scores_uses_correct_stage_thresholds() {
        let p = test_scoring_params();
        // clean = 50.0
        // Stage 1: [80,60,40,20,5]  → fails 80,60; passes 40 → 3 points
        assert_eq!(calc_scores(1, 100.0, 50.0, &p).clean_points, 3);
        // Stage 3: [90,70,50,30,15] → fails 90,70; passes 50 → 3 points
        assert_eq!(calc_scores(3, 100.0, 50.0, &p).clean_points, 3);
        // Stage 5: [95,80,60,40,25] → fails 95,80,60; passes 40 → 2 points
        assert_eq!(calc_scores(5, 100.0, 50.0, &p).clean_points, 2);
    }

    #[test]
    fn calc_scores_stage_beyond_n_clamps_to_last() {
        let p = test_scoring_params();
        let s5 = calc_scores(5, 100.0, 50.0, &p);
        let s6 = calc_scores(6, 100.0, 50.0, &p);
        assert_eq!(s5.clean_points, s6.clean_points);
    }

    #[test]
    fn sample_normal_approximate_mean_and_std() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(12345);
        let n = 50_000usize;
        let mean = 3.0f64;
        let std = 1.5f64;
        let samples: Vec<f64> = (0..n).map(|_| sample_normal(mean, std, &mut rng)).collect();
        let emp_mean = samples.iter().sum::<f64>() / n as f64;
        let emp_var = samples.iter().map(|&x| (x - emp_mean).powi(2)).sum::<f64>() / n as f64;
        assert!((emp_mean - mean).abs() < 0.05, "mean off: {emp_mean}");
        assert!((emp_var.sqrt() - std).abs() < 0.05, "std off: {}", emp_var.sqrt());
    }
}
