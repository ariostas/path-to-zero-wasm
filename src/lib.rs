pub mod data;
pub mod engine;
pub mod screens;
pub mod solver;
pub mod state;
pub mod types;

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use screens::{EndGameScreen, PlanningScreen, SetupScreen, StageResultsScreen};
use state::{GameState, Screen};

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    // None = Setup screen; Some(GameState) = in-game.
    let game: RwSignal<Option<GameState>> = RwSignal::new(None);
    provide_context(game);

    // Route on the current screen only, so that ordinary state changes (e.g.
    // allocating a token) update the page in place instead of rebuilding it.
    let screen = Memo::new(move |_| game.with(|opt| opt.as_ref().map(|gs| gs.screen)));

    view! {
        <header>
            <h1>"Path to Zero"</h1>
            <h2>"The Electricity Decarbonization Game"</h2>
        </header>
        <div class="container">
            {move || match screen.get() {
                None => view! { <SetupScreen /> }.into_any(),
                Some(Screen::Planning) => view! { <PlanningScreen /> }.into_any(),
                Some(Screen::StageResults) => view! { <StageResultsScreen /> }.into_any(),
                Some(Screen::EndGame) => view! { <EndGameScreen /> }.into_any(),
            }}
        </div>
    }
}
