//! Hourly economic dispatch.
//!
//! The original game solves a linear program (see `lp.rs`), which is too slow
//! to run interactively in WASM. [`dispatch`] reproduces the LP's decisions
//! with two fast steps:
//!
//! 1. Non-storage resources are dispatched in merit order (cheapest variable
//!    cost first); demand that cannot be met becomes non-served energy (NSE).
//! 2. Storage is scheduled by dynamic programming over a discretised state of
//!    charge, separately for each representative period. Each hour's cost is
//!    the merit-order cost of serving `demand + charge − discharge`, so the
//!    battery charges from whatever capacity is cheapest (curtailed VRE, spare
//!    nuclear, or gas ahead of a shortfall) and discharges where it avoids
//!    the most expensive generation or NSE.
//!
//! Differences from the LP: minimum-power and ramp constraints are ignored,
//! only the first storage resource is scheduled (the game data has one), and
//! storage energy is quantised to `1 / SOC_STEPS` of its capacity.

#[cfg(test)]
mod lp;

use crate::types::*;

/// Number of state-of-charge steps used by the storage dynamic program.
const SOC_STEPS: usize = 32;

/// Small cost per MWh of storage throughput, so that ties are broken in
/// favour of leaving the battery idle.
const CYCLING_EPSILON: f64 = 1e-6;

pub struct SolverResult {
    /// Generation (MW), indexed [t * n_g + g].
    pub gen: Vec<f64>,
    /// Charging power (MW), indexed [t * n_stor + stor_local].
    pub charge: Vec<f64>,
    /// State of charge (MWh), indexed [t * n_stor + stor_local].
    pub soc: Vec<f64>,
    /// Non-served energy (MW), indexed [t * n_s + s_local].
    pub nse: Vec<f64>,
    pub objective_value: f64,
    pub status: String,
    /// Local indices (into inputs.resources) of storage resources.
    pub stor_indices: Vec<usize>,
}

/// Solve the hourly dispatch for all time steps. See the module docs.
pub fn dispatch(inputs: &SimInputs) -> SolverResult {
    let n_t = inputs.demand.len();
    let n_g = inputs.resources.len();
    let n_s = inputs.nse_segments.len();
    let h = inputs.hours_per_period.max(1);

    let stor_indices: Vec<usize> = inputs
        .resources
        .iter()
        .enumerate()
        .filter(|(_, r)| r.stor >= 1)
        .map(|(i, _)| i)
        .collect();
    let n_stor = stor_indices.len();

    let merit = MeritOrder::new(inputs, &stor_indices);

    let mut gen = vec![0.0f64; n_t * n_g];
    let mut charge = vec![0.0f64; n_t * n_stor];
    let mut soc = vec![0.0f64; n_t * n_stor];
    let mut nse = vec![0.0f64; n_t * n_s];
    let mut objective_value = 0.0;

    let battery = stor_indices.first().map(|&g| &inputs.resources[g]);

    for start in (0..n_t).step_by(h) {
        let end = (start + h).min(n_t);

        // Per-hour (charge MW, discharge MW, SOC MWh after the hour).
        let schedule: Vec<(f64, f64, f64)> = match battery {
            Some(b) if b.existing_cap_mw > 0.0 && b.existing_cap_mwh > 0.0 => {
                StoragePlan::new(inputs, &merit, b, start, end).schedule()
            }
            _ => vec![(0.0, 0.0, 0.0); end - start],
        };

        for (t, &(ch, dis, level)) in (start..end).zip(&schedule) {
            let dis = delivered(dis, inputs.demand[t]);
            let load = inputs.demand[t] + ch - dis;
            objective_value += inputs.sample_weight[t]
                * merit.serve(t, load, Some((&mut gen[t * n_g..(t + 1) * n_g], &mut nse[t * n_s..(t + 1) * n_s])));
            if let (Some(&sg), Some(b)) = (stor_indices.first(), battery) {
                gen[t * n_g + sg] = dis;
                charge[t * n_stor] = ch;
                soc[t * n_stor] = level;
                objective_value += inputs.sample_weight[t] * b.var_cost * dis;
            }
        }
    }

    SolverResult {
        gen,
        charge,
        soc,
        nse,
        objective_value,
        status: "dispatch".to_string(),
        stor_indices,
    }
}

/// Discharge power actually delivered to the grid. SOC moves in whole steps,
/// so a step can exceed the hour's demand; the excess energy is discarded.
fn delivered(discharge: f64, demand: f64) -> f64 {
    discharge.min(demand.max(0.0))
}

/// Merit-order supply curve for serving a given load in a given hour.
struct MeritOrder<'a> {
    inputs: &'a SimInputs,
    /// Non-storage resources sorted by variable cost (cheapest first).
    order: Vec<usize>,
    /// NSE segments sorted by cost (cheapest first).
    nse_order: Vec<usize>,
}

impl<'a> MeritOrder<'a> {
    fn new(inputs: &'a SimInputs, stor_indices: &[usize]) -> Self {
        let mut order: Vec<usize> = (0..inputs.resources.len())
            .filter(|g| !stor_indices.contains(g))
            .collect();
        order.sort_by(|&a, &b| inputs.resources[a].var_cost.total_cmp(&inputs.resources[b].var_cost));
        let mut nse_order: Vec<usize> = (0..inputs.nse_segments.len()).collect();
        nse_order.sort_by(|&a, &b| {
            inputs.nse_segments[a].nse_cost.total_cmp(&inputs.nse_segments[b].nse_cost)
        });
        Self { inputs, order, nse_order }
    }

    /// Unweighted cost ($) of serving `load` MW in hour `t`, optionally
    /// writing the per-resource generation and per-segment NSE (MW).
    ///
    /// Returns `f64::INFINITY` if `load` exceeds generation plus all NSE.
    fn serve(&self, t: usize, load: f64, mut out: Option<(&mut [f64], &mut [f64])>) -> f64 {
        let inputs = self.inputs;
        let n_g = inputs.resources.len();
        let mut remaining = load.max(0.0);
        let mut cost = 0.0;

        for &g in &self.order {
            let res = &inputs.resources[g];
            let available = res.existing_cap_mw * inputs.variability[t * n_g + g];
            let p = available.min(remaining).max(0.0);
            cost += p * res.var_cost;
            remaining -= p;
            if let Some((gen, _)) = out.as_mut() {
                gen[g] = p;
            }
        }
        for &s in &self.nse_order {
            let seg = &inputs.nse_segments[s];
            let p = (seg.nse_max * inputs.demand[t]).min(remaining).max(0.0);
            cost += p * seg.nse_cost;
            remaining -= p;
            if let Some((_, nse)) = out.as_mut() {
                nse[s] = p;
            }
        }
        if remaining > 1e-6 {
            f64::INFINITY
        } else {
            cost
        }
    }
}

/// Dynamic program scheduling one storage resource over one period.
struct StoragePlan {
    /// Energy per SOC step (MWh).
    step_mwh: f64,
    eff_up: f64,
    eff_down: f64,
    /// Largest SOC increase / decrease per hour, in steps.
    max_up: usize,
    max_down: usize,
    /// `hour_cost[t][d]`: cost of hour `t` when SOC changes by
    /// `d - max_down` steps (includes the storage variable cost).
    hour_cost: Vec<Vec<f64>>,
}

impl StoragePlan {
    fn new(inputs: &SimInputs, merit: &MeritOrder, battery: &Resource, start: usize, end: usize) -> Self {
        let step_mwh = battery.existing_cap_mwh / SOC_STEPS as f64;
        let eff_up = battery.eff_up.max(1e-6);
        let eff_down = battery.eff_down.max(1e-6);
        let max_up = ((battery.existing_cap_mw * eff_up / step_mwh + 1e-9).floor() as usize).min(SOC_STEPS);
        let max_down =
            ((battery.existing_cap_mw / eff_down / step_mwh + 1e-9).floor() as usize).min(SOC_STEPS);

        let hour_cost = (start..end)
            .map(|t| {
                (0..=max_up + max_down)
                    .map(|d| {
                        let (ch, dis) = Self::flows(step_mwh, eff_up, eff_down, d as isize - max_down as isize);
                        let dis = delivered(dis, inputs.demand[t]);
                        let load = inputs.demand[t] + ch - dis;
                        merit.serve(t, load, None) + battery.var_cost * dis + CYCLING_EPSILON * (ch + dis)
                    })
                    .collect()
            })
            .collect();

        Self { step_mwh, eff_up, eff_down, max_up, max_down, hour_cost }
    }

    /// Charge and discharge power (MW) for a SOC change of `delta` steps.
    fn flows(step_mwh: f64, eff_up: f64, eff_down: f64, delta: isize) -> (f64, f64) {
        let energy = delta as f64 * step_mwh;
        if delta >= 0 {
            (energy / eff_up, 0.0)
        } else {
            (0.0, -energy * eff_down)
        }
    }

    /// Least-cost SOC path starting at `start_level`, ending at or above
    /// `min_end_level`. Returns the SOC level (in steps) after each hour.
    fn solve(&self, start_level: usize, min_end_level: usize) -> Vec<usize> {
        let n_levels = SOC_STEPS + 1;
        let n_hours = self.hour_cost.len();
        let mut value = vec![f64::INFINITY; n_levels];
        value[start_level] = 0.0;
        // Predecessor level for each (hour, level).
        let mut prev = vec![0u16; n_hours * n_levels];

        for (hour, costs) in self.hour_cost.iter().enumerate() {
            let mut next = vec![f64::INFINITY; n_levels];
            for (level, &v) in value.iter().enumerate() {
                if v.is_infinite() {
                    continue;
                }
                let lo = level.saturating_sub(self.max_down);
                let hi = (level + self.max_up).min(SOC_STEPS);
                for new_level in lo..=hi {
                    let d = new_level + self.max_down - level;
                    let candidate = v + costs[d];
                    if candidate < next[new_level] {
                        next[new_level] = candidate;
                        prev[hour * n_levels + new_level] = level as u16;
                    }
                }
            }
            value = next;
        }

        let mut level = (min_end_level..n_levels)
            .min_by(|&a, &b| value[a].total_cmp(&value[b]))
            .unwrap_or(start_level);
        let mut path = vec![0usize; n_hours];
        for hour in (0..n_hours).rev() {
            path[hour] = level;
            level = prev[hour * n_levels + level] as usize;
        }
        path
    }

    /// Schedule the period, approximating the LP's cyclic boundary condition:
    /// a first pass from empty finds a natural end-of-period SOC; the second
    /// pass starts there and must finish at least as full as it started.
    fn schedule(&self) -> Vec<(f64, f64, f64)> {
        let first = self.solve(0, 0);
        let start_level = first.last().copied().unwrap_or(0);
        let path = self.solve(start_level, start_level);

        let mut level = start_level;
        path.iter()
            .map(|&new_level| {
                let delta = new_level as isize - level as isize;
                level = new_level;
                let (ch, dis) = Self::flows(self.step_mwh, self.eff_up, self.eff_down, delta);
                (ch, dis, new_level as f64 * self.step_mwh)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data;

    fn resource(id: usize, name: &str, cap_mw: f64, var_cost: f64) -> Resource {
        Resource {
            id,
            name: name.to_string(),
            therm: 1,
            stor: 0,
            vre: 0,
            new_build: 1,
            existing_cap_mw: cap_mw,
            existing_cap_mwh: 0.0,
            var_cost,
            min_power: 0.0,
            ramp_up_pct: 1.0,
            ramp_dn_pct: 1.0,
            eff_up: 1.0,
            eff_down: 1.0,
        }
    }

    fn battery(id: usize, cap_mw: f64, eff: f64) -> Resource {
        Resource {
            stor: 1,
            therm: 0,
            existing_cap_mwh: cap_mw * 4.0,
            var_cost: 0.15,
            eff_up: eff,
            eff_down: eff,
            ..resource(id, "battery", cap_mw, 0.0)
        }
    }

    fn inputs(resources: Vec<Resource>, demand: Vec<f64>, variability: Vec<f64>) -> SimInputs {
        let n_t = demand.len();
        SimInputs {
            resources,
            sample_weight: vec![1.0; n_t],
            hours_per_period: n_t,
            demand,
            variability,
            nse_segments: vec![NseSegment { segment: 1, nse_cost: 10_000.0, nse_max: 1.0 }],
        }
    }

    fn total_nse(r: &SolverResult) -> f64 {
        r.nse.iter().sum()
    }

    #[test]
    fn merit_order_prefers_cheaper() {
        let inp = inputs(
            vec![resource(0, "gas", 700.0, 30.0), resource(1, "cheap", 700.0, 5.0)],
            vec![400.0],
            vec![1.0, 1.0],
        );
        let r = dispatch(&inp);
        assert_eq!(r.gen, vec![0.0, 400.0]);
        assert_eq!(total_nse(&r), 0.0);
    }

    #[test]
    fn shortfall_becomes_nse() {
        let inp = inputs(vec![resource(0, "gas", 600.0, 10.0)], vec![1000.0], vec![1.0]);
        let r = dispatch(&inp);
        assert_eq!(r.gen[0], 600.0);
        assert!((total_nse(&r) - 400.0).abs() < 1e-9);
    }

    #[test]
    fn battery_shifts_surplus_solar_to_cover_shortfall() {
        // Hour 0: 1000 MW solar vs 200 MW demand (surplus).
        // Hour 1: no solar, 400 MW demand, no other generation.
        let inp = inputs(
            vec![resource(0, "solar", 1000.0, 0.0), battery(1, 500.0, 1.0)],
            vec![200.0, 400.0],
            vec![1.0, 1.0, 0.0, 1.0],
        );
        let r = dispatch(&inp);
        assert!(r.charge[0] >= 400.0, "charge[0] = {}", r.charge[0]);
        assert!(total_nse(&r) < 1e-6, "nse = {:?}", r.nse);
        // Energy balance holds every hour.
        for t in 0..2 {
            let served = r.gen[t * 2] + r.gen[t * 2 + 1] + r.nse[t] - r.charge[t];
            assert!((served - inp.demand[t]).abs() < 1e-6, "t={t}: {served}");
        }
    }

    #[test]
    fn battery_never_exceeds_power_or_energy_limits() {
        let n_t = 48;
        let demand: Vec<f64> = (0..n_t).map(|t| if t % 24 < 12 { 100.0 } else { 900.0 }).collect();
        let variability: Vec<f64> = (0..n_t).flat_map(|t| [if t % 24 < 12 { 1.0 } else { 0.0 }, 1.0]).collect();
        let inp = inputs(vec![resource(0, "solar", 2000.0, 0.0), battery(1, 300.0, 0.9)], demand, variability);
        let r = dispatch(&inp);
        for t in 0..n_t {
            assert!(r.charge[t] <= 300.0 + 1e-6);
            assert!(r.gen[t * 2 + 1] <= 300.0 + 1e-6);
            assert!(r.soc[t] <= 1200.0 + 1e-6 && r.soc[t] >= 0.0);
            assert!(r.charge[t] == 0.0 || r.gen[t * 2 + 1] == 0.0);
        }
    }

    /// Scores computed from a dispatch: (clean share %, reliability %).
    fn scores(inp: &SimInputs, r: &SolverResult) -> (f64, f64) {
        let n_g = inp.resources.len();
        let n_s = inp.nse_segments.len();
        let (mut gas, mut total, mut nse, mut demand) = (0.0, 0.0, 0.0, 0.0);
        for t in 0..inp.demand.len() {
            let w = inp.sample_weight[t];
            demand += w * inp.demand[t];
            for g in 0..n_g {
                if inp.resources[g].stor == 0 {
                    total += w * r.gen[t * n_g + g];
                }
            }
            gas += w * r.gen[t * n_g];
            nse += w * (0..n_s).map(|s| r.nse[t * n_s + s]).sum::<f64>();
        }
        (100.0 - gas / total * 100.0, (1.0 - nse / demand) * 100.0)
    }

    /// Real game data for `year` with the given capacities (GW, CSV order).
    fn real_inputs(year: u32, caps_gw: [f64; N_RESOURCES]) -> SimInputs {
        let params = ResourceParams {
            names: RESOURCE_ORDER.map(|s| s.to_string()),
            start_capacity: [0.0; N_RESOURCES],
            build_cost: [0.0; N_RESOURCES],
            build_tokens: [0; N_RESOURCES],
        };
        let mut inp = data::load_sim_inputs(year, &params, false);
        for (r, cap) in inp.resources.iter_mut().zip(caps_gw) {
            r.existing_cap_mw = cap * 1000.0;
            if r.stor >= 1 {
                r.existing_cap_mwh = r.existing_cap_mw * 4.0;
            }
        }
        inp
    }

    /// The fast dispatch must closely match the reference LP on real data
    /// once the LP-only constraints (min power, ramps) are relaxed.
    #[test]
    fn matches_lp_on_real_data() {
        let portfolios: [[f64; N_RESOURCES]; 3] = [
            [60.0, 10.0, 45.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            [40.0, 10.0, 90.0, 16.0, 60.0, 20.0, 40.0, 0.0],
            [20.0, 20.0, 150.0, 16.0, 100.0, 30.0, 60.0, 8.0],
        ];
        for caps in portfolios {
            let mut inp = real_inputs(2040, caps);
            for r in &mut inp.resources {
                r.min_power = 0.0;
                r.ramp_up_pct = 1.0;
                r.ramp_dn_pct = 1.0;
            }
            let (clean, rel) = scores(&inp, &dispatch(&inp));
            let (lp_clean, lp_rel) = scores(&inp, &lp::solve(&inp));
            assert!((clean - lp_clean).abs() < 0.5, "{caps:?}: clean {clean} vs LP {lp_clean}");
            assert!((rel - lp_rel).abs() < 0.05, "{caps:?}: reliability {rel} vs LP {lp_rel}");
        }
    }

    /// Run `cargo test --release -- --ignored --nocapture time_real_solve` to benchmark.
    #[test]
    #[ignore]
    fn time_real_solve() {
        use std::time::Instant;

        let caps = [40.0, 10.0, 90.0, 16.0, 60.0, 20.0, 40.0, 0.0];
        let inp = real_inputs(2040, caps);
        println!("Problem: {} time steps, {} resources", inp.demand.len(), inp.resources.len());

        let t0 = Instant::now();
        let r = dispatch(&inp);
        println!("dispatch(): {:.2} ms, scores {:?}", t0.elapsed().as_secs_f64() * 1e3, scores(&inp, &r));

        let t0 = Instant::now();
        let r = lp::solve(&inp);
        println!(
            "lp::solve() {}: {:.0} ms, scores {:?}",
            r.status,
            t0.elapsed().as_secs_f64() * 1e3,
            scores(&inp, &r)
        );
    }
}
