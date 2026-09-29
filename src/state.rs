use crate::engine;
use crate::types::*;

// ---------------------------------------------------------------------------
// Game rules (from the original app.jl)
// ---------------------------------------------------------------------------

/// Maximum number of budget tokens a player can hold.
pub const MAX_BUDGET_TOKENS: i32 = 10;
/// Maximum number of unspent shaping tokens a player can hold.
pub const MAX_SHAPING_TOKENS: i32 = 4;
/// Build tokens gained by spending one budget token.
pub const BUILD_TOKENS_PER_PURCHASE: i32 = 2;
/// Build tokens given up to recover one budget token.
pub const BUILD_TOKENS_PER_SALE: i32 = 3;
/// Final-score points per unspent budget token.
pub const AFFORDABILITY_POINTS_PER_TOKEN: i32 = 3;
/// Final-score penalty per new resource under social backlash at game end.
pub const BACKLASH_PENALTY_POINTS: i32 = 2;

// ---------------------------------------------------------------------------
// Screen enum
// ---------------------------------------------------------------------------

/// Which screen the player is currently on.
/// The Setup screen is represented by `Option<GameState>` being `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Planning,
    StageResults,
    EndGame,
}

// ---------------------------------------------------------------------------
// GameState
// ---------------------------------------------------------------------------

/// Full in-progress game state.  Lives inside `RwSignal<Option<GameState>>`
/// provided at the root; `None` means the player is on the Setup screen.
#[derive(Debug, Clone)]
pub struct GameState {
    pub screen: Screen,
    /// Parsed game setup (mutated in-place as scores and backlash accumulate).
    pub setup: GameSetup,
    /// Mutable per-stage resource parameters.
    pub resource_params: ResourceParams,
    /// Shaping tokens currently active (including committed ones).
    pub shaping_token_state: ShapingTokensState,
    /// Shaping tokens committed in earlier stages; these cannot be undone.
    pub shaping_locked: ShapingTokensState,
    /// Unspent shaping tokens.
    pub shaping_tokens_available: i32,
    /// Budget tokens currently held.
    pub budget_tokens: i32,
    /// Build tokens gained (by purchase) or lost (by sale) this stage.
    pub extra_build_tokens: i32,
    /// Build-token purchases made this stage that can still be undone.
    pub build_token_purchases: i32,
    /// Shaping-token purchases made this stage that can still be undone.
    pub shaping_token_purchases: i32,
    /// One entry per completed stage.
    pub stage_history: Vec<StageResults>,
    pub backlash_history: Vec<SocialBacklash>,
    pub experience_history: Vec<ExperienceResults>,
    /// Most recent preview simulation (cleared whenever allocations change).
    pub preview: Option<SimulationResults>,
    /// Results for the most recently completed stage (shown on StageResults screen).
    pub last_stage_results: Option<(StageResults, SocialBacklash, ExperienceResults)>,
}

impl GameState {
    /// Initialise a fresh game from a parsed setup YAML.
    pub fn new(setup: GameSetup) -> Self {
        let resource_params = resource_params_from_setup(&setup);
        Self {
            screen: Screen::Planning,
            resource_params,
            shaping_token_state: setup.shaping_tokens.clone(),
            shaping_locked: setup.shaping_tokens.clone(),
            shaping_tokens_available: setup.current_stage_shaping_tokens,
            budget_tokens: setup.available_budget_tokens,
            extra_build_tokens: 0,
            build_token_purchases: 0,
            shaping_token_purchases: 0,
            setup,
            stage_history: Vec::new(),
            backlash_history: Vec::new(),
            experience_history: Vec::new(),
            preview: None,
            last_stage_results: None,
        }
    }

    // -----------------------------------------------------------------------
    // Derived accessors
    // -----------------------------------------------------------------------

    /// Current stage number (1-based). Resumed games start at the saved
    /// `current_stage`.
    pub fn current_stage_num(&self) -> usize {
        self.setup.current_stage + self.stage_history.len()
    }

    /// Stage number (1-based) of `stage_history[i]`.
    pub fn history_stage_num(&self, i: usize) -> usize {
        self.setup.current_stage + i
    }

    /// Planning year for the current stage.
    pub fn current_year(&self) -> u32 {
        let idx = (self.current_stage_num() - 1).min(N_STAGES - 1);
        self.setup.stages[idx]
    }

    /// Whether nuclear is a new buildable resource in this setup.
    pub fn is_new_nuclear(&self) -> bool {
        self.setup.resource_blocks
            .iter()
            .find(|b| b.edg_data_name == "nuclear")
            .map(|b| b.new_resource)
            .unwrap_or(false)
    }

    /// Display label for an EDG resource name in this game's region.
    pub fn label(&self, edg_name: &str) -> &'static str {
        match resource_index(edg_name) {
            Some(g) => region_resource_label(RESOURCE_ORDER[g], self.setup.is_wy_setup),
            None => "Unknown",
        }
    }

    /// Integer shaping token counts, ready to pass to the engine.
    pub fn shaping_tokens(&self) -> ShapingTokens {
        shaping_tokens_from_state(&self.shaping_token_state)
    }

    /// Build token budget for the current stage, including purchases/sales.
    pub fn build_token_budget(&self) -> i32 {
        let idx = (self.current_stage_num() - 1).min(N_STAGES - 1);
        self.setup.available_build_tokens[idx] + self.extra_build_tokens
    }

    /// Total build tokens allocated so far this stage.
    pub fn build_tokens_used(&self) -> i32 {
        self.resource_params.build_tokens.iter().sum()
    }

    /// Remaining unallocated build tokens this stage.
    pub fn build_tokens_remaining(&self) -> i32 {
        self.build_token_budget() - self.build_tokens_used()
    }

    /// True if the block at index `g` is locked this stage by social backlash.
    pub fn is_locked(&self, g: usize) -> bool {
        self.setup.resource_blocks[g].social_backlash
    }

    /// True if block `g` is an existing resource that must be retained with
    /// build tokens (rather than a new resource that tokens add to).
    pub fn is_existing(&self, g: usize) -> bool {
        !self.setup.resource_blocks[g].new_resource
    }

    /// True if block `g` is clean firm and Innovation: Clean Firm has not
    /// been committed in an earlier stage.
    pub fn is_clean_firm_unavailable(&self, g: usize) -> bool {
        self.resource_params.names[g] == "clean_firm" && !self.shaping_locked.innovation_clean_firm
    }

    /// Capacity (GW) block `g` will have this stage with the current tokens.
    /// Existing resources retire whatever is not retained with tokens.
    pub fn planned_capacity(&self, g: usize) -> f64 {
        let rp = &self.resource_params;
        let tokens_gw = rp.build_cost[g] * rp.build_tokens[g] as f64;
        let nuclear_kept = rp.names[g] == "nuclear" && self.is_new_nuclear();
        if self.is_existing(g) && !nuclear_kept {
            rp.start_capacity[g].min(tokens_gw)
        } else {
            rp.start_capacity[g] + tokens_gw
        }
    }

    /// Whether another build token can be allocated to block `g`.
    pub fn can_add_token(&self, g: usize) -> bool {
        let rp = &self.resource_params;
        let cost = rp.build_cost[g];
        let within_retain_limit = !self.is_existing(g)
            || (rp.build_tokens[g] + 1) as f64 * cost <= rp.start_capacity[g] + 1e-9;
        self.build_tokens_remaining() > 0
            && !self.is_locked(g)
            && !self.is_clean_firm_unavailable(g)
            && cost > 0.0
            && within_retain_limit
    }

    /// Whether a build token can be removed from block `g`.
    pub fn can_remove_token(&self, g: usize) -> bool {
        self.resource_params.build_tokens[g] > 0 && !self.is_locked(g)
    }

    /// Whether shaping token `kind` can currently be toggled.
    pub fn can_toggle_shaping(&self, kind: ShapingKind) -> bool {
        if self.shaping_locked.get(kind) {
            false
        } else {
            self.shaping_token_state.get(kind) || self.shaping_tokens_available > 0
        }
    }

    pub fn can_buy_build_tokens(&self) -> bool {
        self.budget_tokens >= 1
    }

    pub fn can_undo_buy_build_tokens(&self) -> bool {
        self.build_token_purchases > 0
            && self.budget_tokens < MAX_BUDGET_TOKENS
            && self.build_tokens_remaining() >= BUILD_TOKENS_PER_PURCHASE
    }

    pub fn can_sell_build_tokens(&self) -> bool {
        self.build_tokens_remaining() >= BUILD_TOKENS_PER_SALE && self.budget_tokens < MAX_BUDGET_TOKENS
    }

    pub fn can_buy_shaping_token(&self) -> bool {
        self.budget_tokens >= 1 && self.shaping_tokens_available < MAX_SHAPING_TOKENS
    }

    pub fn can_undo_buy_shaping_token(&self) -> bool {
        self.shaping_token_purchases > 0
            && self.shaping_tokens_available > 0
            && self.budget_tokens < MAX_BUDGET_TOKENS
    }

    /// Whether all stages have been played.
    pub fn is_game_over(&self) -> bool {
        self.current_stage_num() > N_STAGES
    }

    /// Cumulative reliability + clean score across all completed stages.
    pub fn total_score(&self) -> i32 {
        self.setup.reliability_scores.iter().sum::<i32>()
            + self.setup.clean_scores.iter().sum::<i32>()
    }

    /// Points awarded at the end of the game for unspent budget tokens.
    pub fn affordability_points(&self) -> i32 {
        AFFORDABILITY_POINTS_PER_TOKEN * self.budget_tokens
    }

    /// Points deducted at the end of the game for new resources under
    /// social backlash.
    pub fn backlash_penalty(&self) -> i32 {
        let n = self.setup.resource_blocks
            .iter()
            .filter(|b| b.new_resource && b.social_backlash)
            .count() as i32;
        BACKLASH_PENALTY_POINTS * n
    }

    /// Final score: stage points + affordability − backlash penalty.
    pub fn final_score(&self) -> i32 {
        self.total_score() + self.affordability_points() - self.backlash_penalty()
    }

    /// The setup describing this game at the start of the current stage, in
    /// the same format as the setup files, so it can be saved and resumed.
    /// Only meaningful before any tokens are allocated or traded this stage.
    pub fn save_setup(&self) -> GameSetup {
        let mut setup = self.setup.clone();
        setup.current_stage = self.current_stage_num();
        setup.available_budget_tokens = self.budget_tokens;
        setup.current_stage_shaping_tokens = self.shaping_tokens_available;
        setup.shaping_tokens = self.shaping_locked.clone();
        for (g, block) in setup.resource_blocks.iter_mut().enumerate() {
            block.start_capacity = self.resource_params.start_capacity[g];
            block.build_cost = self.resource_params.build_cost[g];
        }
        setup
    }

    // -----------------------------------------------------------------------
    // Player actions (each is a no-op if not currently allowed)
    // -----------------------------------------------------------------------

    pub fn add_token(&mut self, g: usize) {
        if self.can_add_token(g) {
            self.resource_params.build_tokens[g] += 1;
            self.preview = None;
        }
    }

    pub fn remove_token(&mut self, g: usize) {
        if self.can_remove_token(g) {
            self.resource_params.build_tokens[g] -= 1;
            self.preview = None;
        }
    }

    pub fn toggle_shaping(&mut self, kind: ShapingKind) {
        if !self.can_toggle_shaping(kind) {
            return;
        }
        let on = !self.shaping_token_state.get(kind);
        self.shaping_token_state.set(kind, on);
        self.shaping_tokens_available += if on { -1 } else { 1 };
        self.preview = None;
    }

    pub fn buy_build_tokens(&mut self) {
        if self.can_buy_build_tokens() {
            self.budget_tokens -= 1;
            self.extra_build_tokens += BUILD_TOKENS_PER_PURCHASE;
            self.build_token_purchases += 1;
        }
    }

    pub fn undo_buy_build_tokens(&mut self) {
        if self.can_undo_buy_build_tokens() {
            self.budget_tokens += 1;
            self.extra_build_tokens -= BUILD_TOKENS_PER_PURCHASE;
            self.build_token_purchases -= 1;
        }
    }

    pub fn sell_build_tokens(&mut self) {
        if self.can_sell_build_tokens() {
            self.budget_tokens += 1;
            self.extra_build_tokens -= BUILD_TOKENS_PER_SALE;
        }
    }

    pub fn buy_shaping_token(&mut self) {
        if self.can_buy_shaping_token() {
            self.budget_tokens -= 1;
            self.shaping_tokens_available += 1;
            self.shaping_token_purchases += 1;
        }
    }

    pub fn undo_buy_shaping_token(&mut self) {
        if self.can_undo_buy_shaping_token() {
            self.budget_tokens += 1;
            self.shaping_tokens_available -= 1;
            self.shaping_token_purchases -= 1;
        }
    }

    /// Run a preview simulation of the current allocation (no uncertainty).
    pub fn run_preview(&mut self) {
        self.preview = Some(engine::run_simulation(
            self.current_year(),
            &self.resource_params,
            &self.setup.scoring_params,
            &self.shaping_tokens(),
            self.current_stage_num(),
            self.is_new_nuclear(),
        ));
    }

    /// Play the current stage (with uncertainty) and record the results.
    pub fn advance(&mut self) {
        if self.is_game_over() {
            return;
        }
        let (stage_results, backlash, experience) = engine::advance_stage(
            &self.setup,
            self.current_stage_num(),
            &self.resource_params,
            &self.shaping_tokens(),
            self.is_new_nuclear(),
        );
        self.apply_stage_results(stage_results, backlash, experience);
    }

    // -----------------------------------------------------------------------
    // State transitions
    // -----------------------------------------------------------------------

    /// Record completed stage results, update parameters for the next stage,
    /// and advance the screen to StageResults.
    pub fn apply_stage_results(
        &mut self,
        stage_results: StageResults,
        backlash: SocialBacklash,
        experience: ExperienceResults,
    ) {
        // Update next-stage capacities and build costs
        self.resource_params.start_capacity = stage_results.next_start_capacity;
        self.resource_params.build_cost = stage_results.next_build_cost;
        self.resource_params.build_tokens = [0; N_RESOURCES];

        // Social backlash locks a resource for the next stage only.
        for (block, &locked) in self.setup.resource_blocks.iter_mut().zip(&backlash.backlash) {
            block.social_backlash = locked;
        }

        // Shaping tokens used this stage are committed for the rest of the game.
        self.shaping_locked = self.shaping_token_state.clone();
        self.extra_build_tokens = 0;
        self.build_token_purchases = 0;
        self.shaping_token_purchases = 0;

        // Record scores on the setup struct (used for history display)
        let stage_idx = self.current_stage_num() - 1;
        self.setup.reliability_scores[stage_idx] = stage_results.scores.reliability_points;
        self.setup.clean_scores[stage_idx] = stage_results.scores.clean_points;

        self.last_stage_results =
            Some((stage_results.clone(), backlash.clone(), experience.clone()));
        self.stage_history.push(stage_results);
        self.backlash_history.push(backlash);
        self.experience_history.push(experience);
        self.preview = None;

        // Always show results first, even after the final stage.
        self.screen = Screen::StageResults;
    }

    /// Transition from the StageResults screen to the next screen:
    /// Planning if there are more stages, EndGame if the game is over.
    pub fn continue_from_results(&mut self) {
        self.screen = if self.is_game_over() {
            Screen::EndGame
        } else {
            Screen::Planning
        };
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn resource_params_from_setup(setup: &GameSetup) -> ResourceParams {
    ResourceParams {
        names: std::array::from_fn(|i| setup.resource_blocks[i].edg_data_name.clone()),
        start_capacity: std::array::from_fn(|i| setup.resource_blocks[i].start_capacity),
        build_cost: std::array::from_fn(|i| setup.resource_blocks[i].build_cost),
        build_tokens: [0; N_RESOURCES],
    }
}

fn shaping_tokens_from_state(state: &ShapingTokensState) -> ShapingTokens {
    ShapingTokens {
        resilience: i32::from(state.resilience),
        innovation_experience: i32::from(state.innovation_experience),
        innovation_clean_firm: i32::from(state.innovation_clean_firm),
        social_license: i32::from(state.social_license),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data;

    fn us_game() -> GameState {
        let setup = data::parse_game_setup(data::SETUP_US).unwrap();
        GameState::new(setup)
    }

    #[test]
    fn new_game_starts_at_stage_1_planning() {
        let gs = us_game();
        assert_eq!(gs.current_stage_num(), 1);
        assert_eq!(gs.screen, Screen::Planning);
    }

    #[test]
    fn current_year_matches_stages_array() {
        let gs = us_game();
        assert_eq!(gs.current_year(), gs.setup.stages[0]);
    }

    #[test]
    fn resource_params_initialised_from_setup() {
        let gs = us_game();
        for i in 0..N_RESOURCES {
            assert_eq!(
                gs.resource_params.names[i],
                gs.setup.resource_blocks[i].edg_data_name
            );
            assert_eq!(
                gs.resource_params.start_capacity[i],
                gs.setup.resource_blocks[i].start_capacity
            );
            assert_eq!(
                gs.resource_params.build_cost[i],
                gs.setup.resource_blocks[i].build_cost
            );
        }
        assert_eq!(gs.resource_params.build_tokens, [0i32; N_RESOURCES]);
    }

    #[test]
    fn build_token_accounting() {
        let mut gs = us_game();
        let budget = gs.build_token_budget();
        assert!(budget > 0, "expected a positive token budget");

        // Allocate 2 tokens to resource 0
        gs.resource_params.build_tokens[0] = 2;
        assert_eq!(gs.build_tokens_used(), 2);
        assert_eq!(gs.build_tokens_remaining(), budget - 2);
    }

    #[test]
    fn shaping_tokens_conversion() {
        let mut gs = us_game();
        gs.shaping_token_state.resilience = true;
        gs.shaping_token_state.innovation_clean_firm = true;
        let st = gs.shaping_tokens();
        assert_eq!(st.resilience, 1);
        assert_eq!(st.innovation_experience, 0);
        assert_eq!(st.innovation_clean_firm, 1);
        assert_eq!(st.social_license, 0);
    }

    #[test]
    fn total_score_sums_both_categories() {
        let mut gs = us_game();
        gs.setup.reliability_scores = [3, 4, 0, 0, 0];
        gs.setup.clean_scores = [2, 5, 0, 0, 0];
        assert_eq!(gs.total_score(), 14);
    }

    fn block(gs: &GameState, name: &str) -> usize {
        gs.resource_params.names.iter().position(|n| n == name).unwrap()
    }

    #[test]
    fn existing_resources_can_only_retain_starting_capacity() {
        let mut gs = us_game();
        let gas = block(&gs, "natural_gas"); // 60 GW, 20 GW/token
        assert_eq!(gs.planned_capacity(gas), 0.0, "unretained gas retires");
        for _ in 0..5 {
            gs.add_token(gas);
        }
        assert_eq!(gs.resource_params.build_tokens[gas], 3);
        assert_eq!(gs.planned_capacity(gas), 60.0);
    }

    #[test]
    fn new_resources_add_capacity_per_token() {
        let mut gs = us_game();
        let solar = block(&gs, "solar_pv"); // 15 GW/token
        gs.add_token(solar);
        gs.add_token(solar);
        assert_eq!(gs.planned_capacity(solar), 30.0);
        gs.remove_token(solar);
        assert_eq!(gs.planned_capacity(solar), 15.0);
    }

    #[test]
    fn cannot_allocate_beyond_budget() {
        let mut gs = us_game();
        let solar = block(&gs, "solar_pv");
        for _ in 0..50 {
            gs.add_token(solar);
        }
        assert_eq!(gs.build_tokens_used(), gs.build_token_budget());
        assert_eq!(gs.build_tokens_remaining(), 0);
    }

    #[test]
    fn shaping_tokens_are_limited_and_committed_after_a_stage() {
        let mut gs = us_game();
        assert_eq!(gs.shaping_tokens_available, 2);
        for kind in ShapingKind::ALL {
            gs.toggle_shaping(kind);
        }
        // Only the first two could be afforded.
        assert!(gs.shaping_token_state.resilience && gs.shaping_token_state.innovation_experience);
        assert!(!gs.shaping_token_state.innovation_clean_firm && !gs.shaping_token_state.social_license);
        assert_eq!(gs.shaping_tokens_available, 0);

        // Toggling off within the stage refunds the token.
        gs.toggle_shaping(ShapingKind::Resilience);
        assert_eq!(gs.shaping_tokens_available, 1);
        gs.toggle_shaping(ShapingKind::Resilience);

        gs.advance();
        gs.continue_from_results();
        // Committed tokens stay on and cannot be toggled off.
        gs.toggle_shaping(ShapingKind::Resilience);
        assert!(gs.shaping_token_state.resilience);
        assert_eq!(gs.shaping_tokens_available, 0);
    }

    #[test]
    fn clean_firm_requires_committed_innovation_token() {
        let mut gs = us_game();
        let cf = block(&gs, "clean_firm");
        gs.toggle_shaping(ShapingKind::InnovationCleanFirm);
        gs.add_token(cf);
        assert_eq!(gs.resource_params.build_tokens[cf], 0, "not available in the same stage");

        gs.advance();
        gs.continue_from_results();
        gs.add_token(cf);
        assert_eq!(gs.resource_params.build_tokens[cf], 1);
    }

    #[test]
    fn budget_token_trades() {
        let mut gs = us_game();
        let (budget, build) = (gs.budget_tokens, gs.build_token_budget());

        gs.buy_build_tokens();
        assert_eq!((gs.budget_tokens, gs.build_token_budget()), (budget - 1, build + 2));
        gs.undo_buy_build_tokens();
        assert_eq!((gs.budget_tokens, gs.build_token_budget()), (budget, build));
        gs.undo_buy_build_tokens(); // nothing left to undo
        assert_eq!(gs.budget_tokens, budget);

        gs.sell_build_tokens();
        assert_eq!((gs.budget_tokens, gs.build_token_budget()), (budget + 1, build - 3));

        let shaping = gs.shaping_tokens_available;
        gs.buy_shaping_token();
        assert_eq!((gs.budget_tokens, gs.shaping_tokens_available), (budget, shaping + 1));
        gs.undo_buy_shaping_token();
        assert_eq!((gs.budget_tokens, gs.shaping_tokens_available), (budget + 1, shaping));
    }

    #[test]
    fn backlash_locks_for_one_stage_only() {
        let mut gs = us_game();
        let sr = |gs: &GameState| {
            let (sr, _, ex) = crate::engine::advance_stage(
                &gs.setup,
                gs.current_stage_num(),
                &gs.resource_params,
                &gs.shaping_tokens(),
                false,
            );
            (sr, ex)
        };
        let mut backlash = SocialBacklash::default();
        backlash.backlash[0] = true;
        let (r, ex) = sr(&gs);
        gs.apply_stage_results(r, backlash, ex);
        assert!(gs.is_locked(0));
        assert!(!gs.can_add_token(0));

        let (r, ex) = sr(&gs);
        gs.apply_stage_results(r, SocialBacklash::default(), ex);
        assert!(!gs.is_locked(0));
    }

    #[test]
    fn final_score_includes_affordability_and_backlash_penalty() {
        let mut gs = us_game();
        gs.setup.reliability_scores = [5; N_STAGES];
        gs.setup.clean_scores = [4; N_STAGES];
        gs.budget_tokens = 2;
        let solar = block(&gs, "solar_pv");
        let gas = block(&gs, "natural_gas");
        gs.setup.resource_blocks[solar].social_backlash = true;
        gs.setup.resource_blocks[gas].social_backlash = true; // existing: no penalty
        assert_eq!(gs.total_score(), 45);
        assert_eq!(gs.final_score(), 45 + 6 - 2);
    }

    #[test]
    fn saved_game_resumes_where_it_left_off() {
        let mut gs = us_game();
        let gas = block(&gs, "natural_gas");
        for _ in 0..3 {
            gs.add_token(gas);
        }
        gs.buy_shaping_token();
        gs.toggle_shaping(ShapingKind::SocialLicense);
        gs.advance();

        let yaml = data::game_setup_to_yaml(&gs.save_setup());
        let resumed = GameState::new(data::parse_game_setup(&yaml).unwrap());
        assert_eq!(resumed.current_stage_num(), 2);
        assert_eq!(resumed.current_year(), gs.current_year());
        assert_eq!(resumed.resource_params.start_capacity, gs.resource_params.start_capacity);
        assert_eq!(resumed.resource_params.build_cost, gs.resource_params.build_cost);
        assert_eq!(resumed.budget_tokens, gs.budget_tokens);
        assert_eq!(resumed.shaping_tokens_available, gs.shaping_tokens_available);
        assert_eq!(resumed.shaping_locked, gs.shaping_locked);
        assert!(resumed.shaping_locked.social_license);
        assert_eq!(resumed.total_score(), gs.total_score());
        for g in 0..N_RESOURCES {
            assert_eq!(resumed.is_locked(g), gs.is_locked(g));
        }
    }

    #[test]
    fn builtin_setups_round_trip_through_yaml() {
        for (name, yaml) in data::builtin_setups() {
            let setup = data::parse_game_setup(yaml).unwrap();
            let again = data::parse_game_setup(&data::game_setup_to_yaml(&setup)).unwrap();
            assert_eq!(format!("{setup:?}"), format!("{again:?}"), "{name}");
        }
    }
}
