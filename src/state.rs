use crate::types::*;

// ---------------------------------------------------------------------------
// Screen enum
// ---------------------------------------------------------------------------

/// Which screen the player is currently on.
/// The Setup screen is represented by `Option<GameState>` being `None`.
#[derive(Debug, Clone, PartialEq)]
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
    /// Shaping token toggle state; player may adjust each stage.
    pub shaping_token_state: ShapingTokensState,
    /// One entry per completed stage.
    pub stage_history: Vec<StageResults>,
    pub backlash_history: Vec<SocialBacklash>,
    pub experience_history: Vec<ExperienceResults>,
    /// Most recent preview simulation (updated whenever token allocations change).
    pub preview: Option<SimulationResults>,
    /// Results for the most recently completed stage (shown on StageResults screen).
    pub last_stage_results: Option<(StageResults, SocialBacklash, ExperienceResults)>,
}

impl GameState {
    /// Initialise a fresh game from a parsed setup YAML.
    pub fn new(setup: GameSetup) -> Self {
        let resource_params = resource_params_from_setup(&setup);
        let shaping_token_state = setup.shaping_tokens.clone();
        Self {
            screen: Screen::Planning,
            resource_params,
            shaping_token_state,
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

    /// Current stage number (1-based).
    pub fn current_stage_num(&self) -> usize {
        self.stage_history.len() + 1
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

    /// Integer shaping token counts, ready to pass to the engine.
    pub fn shaping_tokens(&self) -> ShapingTokens {
        shaping_tokens_from_state(&self.shaping_token_state)
    }

    /// Build token budget for the current stage.
    pub fn build_token_budget(&self) -> i32 {
        let idx = (self.current_stage_num() - 1).min(N_STAGES - 1);
        self.setup.available_build_tokens[idx]
    }

    /// Total build tokens allocated so far this stage.
    pub fn build_tokens_used(&self) -> i32 {
        self.resource_params.build_tokens.iter().sum()
    }

    /// Remaining unallocated build tokens this stage.
    pub fn build_tokens_remaining(&self) -> i32 {
        self.build_token_budget() - self.build_tokens_used()
    }

    /// True if the resource at index `g` is locked due to social backlash.
    pub fn is_locked(&self, g: usize) -> bool {
        self.setup.resource_blocks[g].social_backlash
    }

    /// Cumulative reliability + clean score across all completed stages.
    pub fn total_score(&self) -> i32 {
        self.setup.reliability_scores.iter().sum::<i32>()
            + self.setup.clean_scores.iter().sum::<i32>()
    }

    // -----------------------------------------------------------------------
    // State transitions
    // -----------------------------------------------------------------------

    /// Record completed stage results, update parameters for the next stage,
    /// and advance the screen to StageResults (or EndGame after stage 5).
    pub fn apply_stage_results(
        &mut self,
        stage_results: StageResults,
        backlash: SocialBacklash,
        experience: ExperienceResults,
    ) {
        // Update next-stage capacities and build costs
        for g in 0..N_RESOURCES {
            self.resource_params.start_capacity[g] = stage_results.next_start_capacity[g];
            self.resource_params.build_cost[g] = stage_results.next_build_cost[g];
        }
        self.resource_params.build_tokens = [0; N_RESOURCES];

        // Propagate social backlash flags into the setup resource blocks
        for g in 0..N_RESOURCES {
            if backlash.backlash[g] {
                self.setup.resource_blocks[g].social_backlash = true;
            }
        }

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

        self.screen = if self.stage_history.len() >= N_STAGES {
            Screen::EndGame
        } else {
            Screen::StageResults
        };
    }

    /// Transition from the StageResults screen back to Planning.
    pub fn continue_to_planning(&mut self) {
        self.screen = Screen::Planning;
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
        let setup = data::parse_game_setup(data::SETUP_US);
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
}
