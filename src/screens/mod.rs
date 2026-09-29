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

/// Offer `text` to the user as a file download named `filename`.
pub(crate) fn download_text(filename: &str, text: &str) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;

    let parts = js_sys::Array::of1(&text.into());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("text/yaml");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &options)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;

    let document = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let anchor: web_sys::HtmlAnchorElement = document.create_element("a")?.dyn_into()?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    web_sys::Url::revoke_object_url(&url)
}
