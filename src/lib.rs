pub mod data;
pub mod engine;
pub mod solver;
pub mod state;
pub mod types;

use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use state::GameState;

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

    view! {
        <header>
            <h1>"Path to Zero"</h1>
            <h2>"The Electricity Decarbonization Game"</h2>
        </header>
        <div class="container">
            <p>"Loading..."</p>
        </div>
    }
}
