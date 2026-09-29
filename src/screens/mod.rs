mod end_game;
mod planning;
mod setup;
mod stage_results;

pub use end_game::EndGameScreen;
pub use planning::PlanningScreen;
pub use setup::SetupScreen;
pub use stage_results::StageResultsScreen;

use leptos::prelude::*;

use crate::state::GameState;

/// The game-state signal provided at the app root.
pub(crate) type GameSignal = RwSignal<Option<GameState>>;

pub(crate) fn use_game() -> GameSignal {
    use_context::<GameSignal>().expect("game context missing")
}

/// Read from the game state, returning `None` when no game is in progress
/// (e.g. while the app is switching back to the setup screen).
pub(crate) fn read<T>(game: GameSignal, f: impl FnOnce(&GameState) -> T) -> Option<T> {
    game.with(|opt| opt.as_ref().map(f))
}

/// Apply a player action to the game state, if a game is in progress.
pub(crate) fn act(game: GameSignal, f: impl FnOnce(&mut GameState)) {
    game.update(|opt| {
        if let Some(gs) = opt {
            f(gs);
        }
    });
}
