# Path to Zero — Rust/WASM Implementation Plan

The data layer (`data.rs`), LP solver (`solver.rs`), and game engine (`engine.rs`) are
complete and fully tested. What remains is wiring everything together in a Leptos UI.

---

## Step 1 — Game state management ✓

Create a central reactive state type that owns the full in-progress game and expose it
as a Leptos context so every component can read and update it.

- Define a `GameState` struct holding:
  - `GameSetup` (parsed from YAML at startup)
  - `ResourceParams` (current stage capacities, build costs, token allocations)
  - `ShapingTokens` (integer counts derived from `ShapingTokensState`)
  - Stage history (`Vec<StageResults>`, `Vec<SocialBacklash>`, `Vec<ExperienceResults>`)
  - `current_screen` enum (`Setup | Planning | Results | EndGame`)
- Wrap it in `RwSignal<GameState>` and provide it at the root with `provide_context`.

---

## Step 2 — Setup screen ✓

The first screen the player sees. Allows choosing a pre-built scenario or uploading a
custom YAML file.

- List the built-in setups from `data::builtin_setups()` as clickable cards.
- On selection, call `data::parse_game_setup(yaml)` and initialise `GameState`.
- Show a brief description of the chosen region (resource mix, token budget, stages).
- Optional: file-input element wired via `web-sys` to accept a custom YAML upload.
- "Start Game" button transitions to the Planning screen (Stage 1).

---

## Step 3 — Stage planning screen ✓

The main gameplay loop, repeated once per stage (5 total).

- Header showing current stage year, tokens remaining, and cumulative score.
- **Build token panel**: for each resource block, a control to allocate build tokens
  (spinner or +/- buttons). Enforce the per-stage budget and block locked resources.
- **Shaping token panel**: toggle switches for Resilience, Innovation (Experience),
  Innovation (Clean Firm), and Social License. Reflect `ShapingTokensState`.
- **Preview button**: calls `engine::run_simulation` with current allocations and
  displays projected scores and generation mix without committing to the stage.
- **Advance button**: calls `engine::advance_stage`, stores the returned `StageResults`,
  `SocialBacklash`, and `ExperienceResults`, then transitions to the Results screen.

---

## Step 4 — Dispatch and results visualization ✓

Shown after advancing each stage; summarises what happened.

- **Stacked area chart** of hourly dispatch using `leptos-chartistry`, with one series
  per resource plus a non-served energy layer. X-axis = hour, Y-axis = GW.
- **Generation mix table**: resource name, GWh, % of total, ending capacity (GW),
  capacity factor (%). Sourced from `Vec<ResourceResult>`.
- **Reliability panel**: reliability %, reserve margin (GW), NSE (GWh and % of demand).
  Sourced from `NseResult`.
- **Scores panel**: reliability points and clean-energy points earned this stage, with
  running totals. Sourced from `StageScores`.

---

## Step 5 — Uncertainty and update narrative ✓

Displayed alongside the Stage Results, describing the random events that occurred.

- **Demand shock**: show the % shock (positive or negative) applied to demand.
- **Climate disaster**: if `UncertaintyResult::disaster` is true, describe the event
  (week number, which resources were forced out via `forced_outages`).
- **Social backlash**: list any resources locked for future stages (`SocialBacklash`).
- **Experience curve**: show cost reductions realised this stage (`ExperienceResults`).
- "Continue" button: updates `ResourceParams` with `next_start_capacity` and
  `next_build_cost`, increments stage, and transitions back to the Planning screen
  (or to the End Game screen after stage 5).

---

## Step 6 — End-game summary ✓

Shown after all 5 stages have been completed.

- Total score (sum of reliability + clean-energy points across all stages).
- Per-stage breakdown table: year, reliability points, clean-energy points, subtotal.
- Generation mix evolution: how the capacity of each resource changed stage by stage
  (sourced from `cap_built` in each stage's `ResourceBlock`).
- "Play Again" button to return to the Setup screen and reset state.

---

## Solver optimizations

The original Clarabel LP on the full dataset was too slow for WASM (~15 s).
Two optimizations were applied; a third is documented for future use.

### Option A — Skip non-binding ramp constraints ✓

Resources with `ramp_up_pct = ramp_dn_pct = 1.0` (solar PV, distributed solar,
onshore wind, offshore wind, battery) can never violate a ramp constraint, so
their rows are provably redundant. Only natural gas, nuclear, and clean firm
(ramp pct 0.2–0.5) retain ramp rows. Reduces ramp rows by ~62%.

### Option B — Subsample time steps 24× ✓

Keep one time step per 24 hours within each representative period (7 per week
instead of 168), multiplying each sampled step's `sample_weight` by 24 to
preserve annual energy totals. `hours_per_period` shrinks from 168 to 7.
Reduces the LP from 8 735 to 364 time steps. Controlled by `STRIDE` in
`data.rs`.

**Combined result (A + B):** native release solve ~27 ms (was ~15 s);
expect ~60–150 ms in WASM.

### Option C — Merit-order dispatch (if LP is still too slow)

Replace the Clarabel LP entirely with an O(n_t × n_g) heuristic:

1. Sort non-storage generators by variable cost (merit order).
2. Each hour: dispatch cheapest first up to available capacity (VRE scaled by
   variability), accumulating unmet demand.
3. Battery: charge when surplus exists after step 2; discharge against deficit.
4. Remaining unmet demand → NSE.

This is essentially what the LP returns for resources with no binding ramp
constraints, so results should be nearly identical for the current dataset.
Expected solve time: < 1 ms. To implement, add `merit_order_solve` in
`solver.rs` and swap the two `solver::solve` calls in `engine.rs`.

---

## Step 7 — Polish and optional features

- Responsive CSS layout that works on tablets (primary target for classroom use).
- Tooltips or an info panel for each resource block (using `edg_data_info`).
- Color-coded scoring feedback (green/yellow/red) on the reliability and clean panels.
- Accessible form controls (ARIA labels, keyboard navigation).
- Custom YAML upload via `web-sys` `FileReader` API (if not done in Step 2).
- Persist in-progress game to `localStorage` via `web-sys` so the page can be
  refreshed without losing progress.
