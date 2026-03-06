use clarabel::algebra::*;
use clarabel::solver::*;

use crate::types::*;

pub struct SolverResult {
    /// Unshifted generation (MW), indexed [t * N_RESOURCES + g].
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

/// Solve the LP economic dispatch using Clarabel.
///
/// Maps the JuMP/HiGHS model from EDG_engine.jl to Clarabel's standard form:
///   min  q'x + (1/2) x'Px
///   s.t. Ax + s = b,  s ∈ K
///
/// Equality cone (ZeroConeT):   Ax = b   (demand balance, SOC dynamics)
/// Nonneg cone (NonnegativeConeT): Ax ≤ b (bounds, ramp limits)
///
/// Variable shifting: vGEN_shifted[t,g] = vGEN[t,g] - lb[g]
/// where lb[g] = min_power[g] * cap[g], so all LP variables ≥ 0.
pub fn solve(inputs: &SimInputs) -> SolverResult {
    let n_t = inputs.demand.len();
    let n_g = inputs.resources.len();
    let h = inputs.hours_per_period;
    let n_s = inputs.nse_segments.len();

    // Identify storage resources in resource order
    let stor_indices: Vec<usize> = inputs
        .resources
        .iter()
        .enumerate()
        .filter(|(_, r)| r.stor >= 1)
        .map(|(i, _)| i)
        .collect();
    let n_stor = stor_indices.len();

    // Variable layout (flat offset into the LP variable vector)
    let gen_off = 0usize;
    let charge_off = n_t * n_g;
    let soc_off = n_t * (n_g + n_stor);
    let nse_off = n_t * (n_g + 2 * n_stor);
    let n_vars = n_t * (n_g + 2 * n_stor + n_s);

    // Lower bounds for generation (for variable shifting)
    let lb: Vec<f64> = inputs
        .resources
        .iter()
        .map(|r| r.min_power * r.existing_cap_mw)
        .collect();

    // Period structure (0-indexed)
    let n_periods = n_t / h;
    let n_interior = n_t - n_periods; // time steps not starting a period

    // -----------------------------------------------------------------------
    // Objective vector q
    // -----------------------------------------------------------------------
    let mut q = vec![0.0f64; n_vars];
    for t in 0..n_t {
        let w = inputs.sample_weight[t];
        for g in 0..n_g {
            q[gen_off + t * n_g + g] = w * inputs.resources[g].var_cost;
        }
        for (s_local, seg) in inputs.nse_segments.iter().enumerate() {
            q[nse_off + t * n_s + s_local] = w * seg.nse_cost;
        }
    }

    // -----------------------------------------------------------------------
    // Constraint structure
    // -----------------------------------------------------------------------

    // Equality block offsets
    let eq_demand_off = 0usize; // n_t rows
    let eq_soc_off = n_t; // n_t * n_stor rows
    let n_eq = n_t + n_t * n_stor;

    // Inequality block offsets (all Ax ≤ b)
    let iq_neg_gen = 0usize; // n_t * n_g
    let iq_neg_charge = iq_neg_gen + n_t * n_g; // n_t * n_stor
    let iq_neg_soc = iq_neg_charge + n_t * n_stor; // n_t * n_stor
    let iq_neg_nse = iq_neg_soc + n_t * n_stor; // n_t * n_s
    let iq_max_power = iq_neg_nse + n_t * n_s; // n_t * n_g
    let iq_max_charge = iq_max_power + n_t * n_g; // n_t * n_stor
    let iq_max_soc = iq_max_charge + n_t * n_stor; // n_t * n_stor
    let iq_max_nse = iq_max_soc + n_t * n_stor; // n_t * n_s
    let iq_ramp_up_int = iq_max_nse + n_t * n_s; // n_interior * n_g
    let iq_ramp_up_wrap = iq_ramp_up_int + n_interior * n_g; // n_periods * n_g
    let iq_ramp_dn_int = iq_ramp_up_wrap + n_periods * n_g; // n_interior * n_g
    let iq_ramp_dn_wrap = iq_ramp_dn_int + n_interior * n_g; // n_periods * n_g
    let n_ineq = iq_ramp_dn_wrap + n_periods * n_g;

    let n_rows = n_eq + n_ineq;
    let ib = n_eq; // inequality row base offset

    // Rough estimate for triplet capacity
    let capacity_guess = n_rows * 4;
    let mut triplets: Vec<(usize, usize, f64)> = Vec::with_capacity(capacity_guess);
    let mut b = vec![0.0f64; n_rows];

    let sum_lb: f64 = lb.iter().sum();

    // -----------------------------------------------------------------------
    // Equality constraints
    // -----------------------------------------------------------------------

    // --- Demand balance: row = eq_demand_off + t ---
    // Σ_g vGEN_shifted[t,g] + Σ_s vNSE[t,s] - Σ_stor vCHARGE[t,stor] = demand[t] - Σ_g lb[g]
    for t in 0..n_t {
        let row = eq_demand_off + t;
        for g in 0..n_g {
            triplets.push((row, gen_off + t * n_g + g, 1.0));
        }
        for s_local in 0..n_s {
            triplets.push((row, nse_off + t * n_s + s_local, 1.0));
        }
        for stor_local in 0..n_stor {
            triplets.push((row, charge_off + t * n_stor + stor_local, -1.0));
        }
        b[row] = inputs.demand[t] - sum_lb;
    }

    // --- SOC dynamics: row = eq_soc_off + t * n_stor + stor_local ---
    // Interior (t % h != 0):  vSOC[t] - vSOC[t-1] - eff_up*vCHARGE[t] + vGEN_shifted[t,stor_g]/eff_down
    //   = -lb[stor_g]/eff_down
    // Wrap    (t % h == 0):   vSOC[t] - vSOC[t+h-1] - eff_up*vCHARGE[t] + vGEN_shifted[t,stor_g]/eff_down
    //   = -lb[stor_g]/eff_down
    for t in 0..n_t {
        for (stor_local, &stor_g) in stor_indices.iter().enumerate() {
            let row = eq_soc_off + t * n_stor + stor_local;
            let res = &inputs.resources[stor_g];
            let eff_up = res.eff_up;
            let eff_down = if res.eff_down > 0.0 { res.eff_down } else { 1.0 };
            let prev_t = if t % h == 0 { t + h - 1 } else { t - 1 };

            triplets.push((row, soc_off + t * n_stor + stor_local, 1.0));
            triplets.push((row, soc_off + prev_t * n_stor + stor_local, -1.0));
            triplets.push((row, charge_off + t * n_stor + stor_local, -eff_up));
            triplets.push((row, gen_off + t * n_g + stor_g, 1.0 / eff_down));
            b[row] = -lb[stor_g] / eff_down;
        }
    }

    // -----------------------------------------------------------------------
    // Inequality constraints (Ax ≤ b, encoded as NonnegativeCone: s=b-Ax ≥ 0)
    // -----------------------------------------------------------------------

    // 1. Nonnegativity vGEN_shifted: -vGEN_shifted[t,g] ≤ 0
    for t in 0..n_t {
        for g in 0..n_g {
            let row = ib + iq_neg_gen + t * n_g + g;
            triplets.push((row, gen_off + t * n_g + g, -1.0));
            // b[row] = 0.0
        }
    }

    // 2. Nonnegativity vCHARGE: -vCHARGE[t,stor_local] ≤ 0
    for t in 0..n_t {
        for stor_local in 0..n_stor {
            let row = ib + iq_neg_charge + t * n_stor + stor_local;
            triplets.push((row, charge_off + t * n_stor + stor_local, -1.0));
        }
    }

    // 3. Nonnegativity vSOC: -vSOC[t,stor_local] ≤ 0
    for t in 0..n_t {
        for stor_local in 0..n_stor {
            let row = ib + iq_neg_soc + t * n_stor + stor_local;
            triplets.push((row, soc_off + t * n_stor + stor_local, -1.0));
        }
    }

    // 4. Nonnegativity vNSE: -vNSE[t,s_local] ≤ 0
    for t in 0..n_t {
        for s_local in 0..n_s {
            let row = ib + iq_neg_nse + t * n_s + s_local;
            triplets.push((row, nse_off + t * n_s + s_local, -1.0));
        }
    }

    // 5. Max power: vGEN_shifted[t,g] ≤ variability[t,g] * cap[g] - lb[g]
    for t in 0..n_t {
        for g in 0..n_g {
            let row = ib + iq_max_power + t * n_g + g;
            let cap = inputs.resources[g].existing_cap_mw;
            let var = inputs.variability[t * n_g + g];
            triplets.push((row, gen_off + t * n_g + g, 1.0));
            b[row] = var * cap - lb[g];
        }
    }

    // 6. Max charge: vCHARGE[t,stor_local] ≤ cap_mw[stor_g]
    for t in 0..n_t {
        for (stor_local, &stor_g) in stor_indices.iter().enumerate() {
            let row = ib + iq_max_charge + t * n_stor + stor_local;
            triplets.push((row, charge_off + t * n_stor + stor_local, 1.0));
            b[row] = inputs.resources[stor_g].existing_cap_mw;
        }
    }

    // 7. Max SOC: vSOC[t,stor_local] ≤ cap_mwh[stor_g]
    for t in 0..n_t {
        for (stor_local, &stor_g) in stor_indices.iter().enumerate() {
            let row = ib + iq_max_soc + t * n_stor + stor_local;
            triplets.push((row, soc_off + t * n_stor + stor_local, 1.0));
            b[row] = inputs.resources[stor_g].existing_cap_mwh;
        }
    }

    // 8. Max NSE: vNSE[t,s_local] ≤ nse_max[s] * demand[t]
    for t in 0..n_t {
        for (s_local, seg) in inputs.nse_segments.iter().enumerate() {
            let row = ib + iq_max_nse + t * n_s + s_local;
            triplets.push((row, nse_off + t * n_s + s_local, 1.0));
            b[row] = seg.nse_max * inputs.demand[t];
        }
    }

    // 9+11. Ramp interior (t % h != 0)
    // Ramp up:   vGEN_shifted[t,g] - vGEN_shifted[t-1,g] ≤ ramp_up * cap[g]
    // Ramp down: vGEN_shifted[t-1,g] - vGEN_shifted[t,g] ≤ ramp_dn * cap[g]
    let mut int_idx = 0usize;
    for t in 0..n_t {
        if t % h == 0 {
            continue;
        }
        for g in 0..n_g {
            let cap = inputs.resources[g].existing_cap_mw;

            let row_up = ib + iq_ramp_up_int + int_idx * n_g + g;
            triplets.push((row_up, gen_off + t * n_g + g, 1.0));
            triplets.push((row_up, gen_off + (t - 1) * n_g + g, -1.0));
            b[row_up] = inputs.resources[g].ramp_up_pct * cap;

            let row_dn = ib + iq_ramp_dn_int + int_idx * n_g + g;
            triplets.push((row_dn, gen_off + (t - 1) * n_g + g, 1.0));
            triplets.push((row_dn, gen_off + t * n_g + g, -1.0));
            b[row_dn] = inputs.resources[g].ramp_dn_pct * cap;
        }
        int_idx += 1;
    }

    // 10+12. Ramp wrap (period starts: t = k*h for k = 0..n_periods-1)
    // Ramp up:   vGEN_shifted[t,g] - vGEN_shifted[t+h-1,g] ≤ ramp_up * cap[g]
    // Ramp down: vGEN_shifted[t+h-1,g] - vGEN_shifted[t,g] ≤ ramp_dn * cap[g]
    for k in 0..n_periods {
        let t = k * h;
        let wrap_t = t + h - 1;
        for g in 0..n_g {
            let cap = inputs.resources[g].existing_cap_mw;

            let row_up = ib + iq_ramp_up_wrap + k * n_g + g;
            triplets.push((row_up, gen_off + t * n_g + g, 1.0));
            triplets.push((row_up, gen_off + wrap_t * n_g + g, -1.0));
            b[row_up] = inputs.resources[g].ramp_up_pct * cap;

            let row_dn = ib + iq_ramp_dn_wrap + k * n_g + g;
            triplets.push((row_dn, gen_off + wrap_t * n_g + g, 1.0));
            triplets.push((row_dn, gen_off + t * n_g + g, -1.0));
            b[row_dn] = inputs.resources[g].ramp_dn_pct * cap;
        }
    }

    // -----------------------------------------------------------------------
    // Build and solve the LP
    // -----------------------------------------------------------------------

    let A = triplets_to_csc(n_rows, n_vars, triplets);

    // P = 0 (no quadratic term)
    let P: CscMatrix<f64> = CscMatrix {
        m: n_vars,
        n: n_vars,
        colptr: vec![0usize; n_vars + 1],
        rowval: vec![],
        nzval: vec![],
    };

    let cones: Vec<SupportedConeT<f64>> = vec![
        SupportedConeT::ZeroConeT(n_eq),
        SupportedConeT::NonnegativeConeT(n_ineq),
    ];

    let settings = DefaultSettings::<f64> {
        verbose: false,
        ..Default::default()
    };

    let mut solver = DefaultSolver::new(&P, &q, &A, &b, &cones, settings)
        .expect("Clarabel solver construction failed");
    solver.solve();
    let sol = solver.solution;
    let x = &sol.x;

    // Unshift generation: vGEN[t,g] = vGEN_shifted[t,g] + lb[g]
    let mut gen = vec![0.0f64; n_t * n_g];
    for t in 0..n_t {
        for g in 0..n_g {
            gen[t * n_g + g] = x[gen_off + t * n_g + g] + lb[g];
        }
    }

    let charge: Vec<f64> = (0..n_t * n_stor).map(|i| x[charge_off + i]).collect();
    let soc: Vec<f64> = (0..n_t * n_stor).map(|i| x[soc_off + i]).collect();
    let nse: Vec<f64> = (0..n_t * n_s).map(|i| x[nse_off + i]).collect();

    SolverResult {
        gen,
        charge,
        soc,
        nse,
        objective_value: sol.obj_val,
        status: format!("{:?}", sol.status),
        stor_indices,
    }
}

/// Build a CSC matrix from unsorted (row, col, val) triplets.
/// Duplicate (row, col) entries are summed.
fn triplets_to_csc(m: usize, n: usize, mut entries: Vec<(usize, usize, f64)>) -> CscMatrix<f64> {
    // Sort column-major, then by row within each column
    entries.sort_unstable_by_key(|&(r, c, _)| (c, r));

    // Merge duplicate (row, col) entries
    let mut merged: Vec<(usize, usize, f64)> = Vec::with_capacity(entries.len());
    for (r, c, v) in entries {
        if let Some(last) = merged.last_mut() {
            if last.0 == r && last.1 == c {
                last.2 += v;
                continue;
            }
        }
        merged.push((r, c, v));
    }

    let mut colptr = vec![0usize; n + 1];
    for &(_, c, _) in &merged {
        colptr[c + 1] += 1;
    }
    for j in 0..n {
        colptr[j + 1] += colptr[j];
    }

    let rowval: Vec<usize> = merged.iter().map(|&(r, _, _)| r).collect();
    let nzval: Vec<f64> = merged.iter().map(|&(_, _, v)| v).collect();

    CscMatrix {
        m,
        n,
        colptr,
        rowval,
        nzval,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn thermal(id: usize, cap_mw: f64, var_cost: f64) -> Resource {
        Resource {
            id,
            name: format!("gen_{id}"),
            therm: 1, stor: 0, vre: 0, new_build: 1,
            existing_cap_mw: cap_mw,
            existing_cap_mwh: 0.0,
            var_cost,
            min_power: 0.0,
            ramp_up_pct: 1.0,
            ramp_dn_pct: 1.0,
            eff_up: 0.0,
            eff_down: 0.0,
        }
    }

    fn nse() -> NseSegment {
        NseSegment { segment: 1, nse_cost: 9000.0, nse_max: 1.0 }
    }

    // -----------------------------------------------------------------------
    // triplets_to_csc
    // -----------------------------------------------------------------------

    #[test]
    fn csc_diagonal() {
        // 2×2 diagonal: (0,0,1), (1,1,2)
        let mat = triplets_to_csc(2, 2, vec![(0, 0, 1.0), (1, 1, 2.0)]);
        assert_eq!(mat.colptr, vec![0, 1, 2]);
        assert_eq!(mat.rowval, vec![0, 1]);
        assert_eq!(mat.nzval, vec![1.0, 2.0]);
    }

    #[test]
    fn csc_merges_duplicates() {
        // Two entries at (0,0): should sum to 3.0
        let mat = triplets_to_csc(2, 2, vec![(0, 0, 1.0), (0, 0, 2.0)]);
        assert_eq!(mat.colptr, vec![0, 1, 1]);
        assert_eq!(mat.rowval, vec![0]);
        assert!((mat.nzval[0] - 3.0).abs() < 1e-12);
    }

    #[test]
    fn csc_unsorted_input() {
        // Entries supplied out of column-major order
        let entries = vec![(1, 1, 4.0), (0, 0, 1.0), (1, 0, 2.0), (0, 1, 3.0)];
        let mat = triplets_to_csc(2, 2, entries);
        // Col 0: rows [0,1] vals [1,2]; Col 1: rows [0,1] vals [3,4]
        assert_eq!(mat.colptr, vec![0, 2, 4]);
        assert_eq!(mat.rowval, vec![0, 1, 0, 1]);
        assert_eq!(mat.nzval, vec![1.0, 2.0, 3.0, 4.0]);
    }

    // -----------------------------------------------------------------------
    // solve
    // -----------------------------------------------------------------------

    #[test]
    fn solve_single_timestep_fully_served() {
        // 1 hour, 1 thermal resource (cap=1000 MW), demand=500 MW → no NSE
        let inputs = SimInputs {
            resources: vec![thermal(0, 1000.0, 10.0)],
            demand: vec![500.0],
            variability: vec![1.0],
            sample_weight: vec![1.0],
            hours_per_period: 1,
            nse_segments: vec![nse()],
        };
        let r = solve(&inputs);
        assert!(r.status.contains("Solved"), "status: {}", r.status);
        assert!((r.gen[0] - 500.0).abs() < 1.0, "gen[0]={}", r.gen[0]);
        assert!(r.nse[0] < 1.0, "nse[0]={}", r.nse[0]);
        assert!((r.objective_value - 5000.0).abs() < 100.0, "obj={}", r.objective_value);
    }

    #[test]
    fn solve_capacity_shortfall_triggers_nse() {
        // Demand (1000 MW) exceeds capacity (600 MW) → NSE fills the gap
        let inputs = SimInputs {
            resources: vec![thermal(0, 600.0, 10.0)],
            demand: vec![1000.0],
            variability: vec![1.0],
            sample_weight: vec![1.0],
            hours_per_period: 1,
            nse_segments: vec![nse()],
        };
        let r = solve(&inputs);
        assert!(r.status.contains("Solved"), "status: {}", r.status);
        assert!((r.gen[0] - 600.0).abs() < 1.0, "gen[0]={}", r.gen[0]);
        assert!((r.nse[0] - 400.0).abs() < 1.0, "nse[0]={}", r.nse[0]);
    }

    #[test]
    fn solve_two_generators_prefers_cheaper() {
        // Demand = 400 MW; two generators, cost 5 vs 30 $/MWh
        // Optimal: all generation from cheaper resource
        let inputs = SimInputs {
            resources: vec![thermal(0, 700.0, 5.0), thermal(1, 700.0, 30.0)],
            demand: vec![400.0],
            variability: vec![1.0, 1.0],
            sample_weight: vec![1.0],
            hours_per_period: 1,
            nse_segments: vec![nse()],
        };
        let r = solve(&inputs);
        assert!(r.status.contains("Solved"), "status: {}", r.status);
        assert!((r.gen[0] - 400.0).abs() < 1.0, "gen[0]={}", r.gen[0]);
        assert!(r.gen[1] < 1.0, "gen[1]={} should be ~0", r.gen[1]);
    }

    #[test]
    fn solve_ramp_constraint_is_respected() {
        // 1 period of 2 hours; ramp limit = 0.1 × 1000 MW = 100 MW/hour
        // demand = [500, 600] → optimal gen = [500, 600] (ramp = exactly 100)
        let mut r0 = thermal(0, 1000.0, 10.0);
        r0.ramp_up_pct = 0.1;
        r0.ramp_dn_pct = 0.1;
        let inputs = SimInputs {
            resources: vec![r0],
            demand: vec![500.0, 600.0],
            variability: vec![1.0, 1.0],
            sample_weight: vec![0.5, 0.5],
            hours_per_period: 2,
            nse_segments: vec![nse()],
        };
        let r = solve(&inputs);
        assert!(r.status.contains("Solved"), "status: {}", r.status);
        assert!((r.gen[0] - 500.0).abs() < 1.0, "gen[0]={}", r.gen[0]);
        assert!((r.gen[1] - 600.0).abs() < 1.0, "gen[1]={}", r.gen[1]);
        // Ramp from t=0 to t=1 must not exceed 100 MW
        assert!(r.gen[1] - r.gen[0] <= 100.0 + 1.0, "ramp={}", r.gen[1] - r.gen[0]);
    }

    #[test]
    fn solve_storage_shifts_load_no_nse() {
        // 2-hour period; without storage, peak demand (800 MW) would require NSE.
        // Battery (cap=300 MW, MWh=1200, 100% efficiency) charges at t=0 and
        // discharges at t=1, allowing flat generation and zero NSE.
        let gen = thermal(0, 700.0, 10.0);
        let bat = Resource {
            id: 1,
            name: "battery".to_string(),
            therm: 0, stor: 1, vre: 0, new_build: 1,
            existing_cap_mw: 300.0,
            existing_cap_mwh: 1200.0,
            var_cost: 0.0,
            min_power: 0.0,
            ramp_up_pct: 1.0,
            ramp_dn_pct: 1.0,
            eff_up: 1.0,
            eff_down: 1.0,
        };
        let inputs = SimInputs {
            resources: vec![gen, bat],
            demand: vec![200.0, 800.0],
            variability: vec![1.0, 1.0, 1.0, 1.0], // [t0g0, t0g1, t1g0, t1g1]
            sample_weight: vec![0.5, 0.5],
            hours_per_period: 2,
            nse_segments: vec![nse()],
        };
        let r = solve(&inputs);
        assert!(r.status.contains("Solved"), "status: {}", r.status);
        assert!(r.stor_indices.contains(&1), "battery should be a storage resource");
        // Both time steps served without curtailment
        assert!(r.nse[0] < 1.0, "nse[0]={}", r.nse[0]);
        assert!(r.nse[1] < 1.0, "nse[1]={}", r.nse[1]);
    }
}
