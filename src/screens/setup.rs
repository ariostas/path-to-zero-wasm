use leptos::prelude::*;

use crate::data::{builtin_setups, parse_game_setup};
use crate::state::GameState;
use crate::types::GameSetup;

// ---------------------------------------------------------------------------
// Display metadata for each built-in setup
// ---------------------------------------------------------------------------

fn display_name(filename: &str) -> &str {
    match filename {
        "US_setup.yml" => "United States",
        "CA_setup.yml" => "California",
        "IL_setup.yml" => "Illinois",
        "NJ_setup.yml" => "New Jersey",
        "WY_setup.yml" => "Wyoming",
        "US_setup_1.yml" => "United States (Variant 1)",
        "US_setup_2.yml" => "United States (Variant 2)",
        _ => filename,
    }
}

fn short_code(filename: &str) -> &str {
    match filename {
        "US_setup.yml" => "US",
        "CA_setup.yml" => "CA",
        "IL_setup.yml" => "IL",
        "NJ_setup.yml" => "NJ",
        "WY_setup.yml" => "WY",
        "US_setup_1.yml" => "US1",
        "US_setup_2.yml" => "US2",
        _ => "?",
    }
}

// ---------------------------------------------------------------------------
// SetupScreen component
// ---------------------------------------------------------------------------

#[component]
pub fn SetupScreen() -> impl IntoView {
    let game = use_context::<RwSignal<Option<GameState>>>()
        .expect("game context missing");

    let selected: RwSignal<Option<usize>> = RwSignal::new(None);

    // Parse all built-in setups once at component creation (static data).
    let setups: StoredValue<Vec<(String, String, GameSetup)>> = StoredValue::new(
        builtin_setups()
            .into_iter()
            .map(|(filename, yaml)| {
                (
                    display_name(filename).to_string(),
                    short_code(filename).to_string(),
                    parse_game_setup(yaml),
                )
            })
            .collect(),
    );

    // ---- Setup cards (static list; only the selected class is reactive) ----
    let cards = setups.with_value(|s| {
        s.iter()
            .enumerate()
            .map(|(i, (name, code, setup))| {
                let name = name.clone();
                let code = code.clone();
                let year_range =
                    format!("{} – {}", setup.stages[0], setup.stages[setup.stages.len() - 1]);
                let total_tokens: i32 = setup.available_build_tokens.iter().sum();

                view! {
                    <button
                        class="setup-card"
                        class:selected=move || selected.get() == Some(i)
                        on:click=move |_| selected.set(Some(i))
                    >
                        <span class="setup-card-code">{code}</span>
                        <span class="setup-card-name">{name}</span>
                        <span class="setup-card-meta">{year_range}</span>
                        <span class="setup-card-meta">{total_tokens}" build tokens"</span>
                    </button>
                }
            })
            .collect_view()
    });

    // ---- Preview panel (reactive on selection) ----
    let preview = move || {
        selected.get().map(|idx| {
            setups.with_value(|s| {
                let (name, _, setup) = &s[idx];
                let name = name.clone();

                let token_rows: Vec<_> = setup
                    .stages
                    .iter()
                    .zip(setup.available_build_tokens.iter())
                    .enumerate()
                    .map(|(i, (year, tokens))| {
                        let label = format!("Stage {} ({})", i + 1, year);
                        let val = format!("{tokens} tokens");
                        view! {
                            <tr>
                                <td>{label}</td>
                                <td class="text-center">{val}</td>
                            </tr>
                        }
                    })
                    .collect();

                let resource_rows: Vec<_> = setup
                    .resource_blocks
                    .iter()
                    .filter(|b| b.start_capacity > 0.0)
                    .map(|b| {
                        let name = b.name.clone();
                        let cap = format!("{:.1} GW", b.start_capacity);
                        view! {
                            <tr>
                                <td>{name}</td>
                                <td class="text-center">{cap}</td>
                            </tr>
                        }
                    })
                    .collect();

                let shaping = setup.available_shaping_tokens;
                let budget = setup.available_budget_tokens;

                view! {
                    <div class="setup-preview">
                        <h3>{name}</h3>

                        <h4>"Build Token Budget"</h4>
                        <table class="score-table mb-16">
                            <thead>
                                <tr><th>"Stage"</th><th>"Tokens"</th></tr>
                            </thead>
                            <tbody>{token_rows}</tbody>
                        </table>

                        <h4>"Starting Capacities"</h4>
                        <table class="score-table mb-16">
                            <thead>
                                <tr><th>"Resource"</th><th>"GW"</th></tr>
                            </thead>
                            <tbody>{resource_rows}</tbody>
                        </table>

                        <p class="mb-8">
                            <strong>"Shaping token budget: "</strong>
                            {shaping}
                        </p>
                        <p>
                            <strong>"Budget tokens: "</strong>
                            {budget}
                        </p>
                    </div>
                }
            })
        })
    };

    // ---- Start game action ----
    let start_disabled = move || selected.get().is_none();

    let start_game = move |_| {
        if let Some(idx) = selected.get() {
            let setup = setups.with_value(|s| s[idx].2.clone());
            game.set(Some(GameState::new(setup)));
        }
    };

    // ---- View ----
    view! {
        <div class="setup-screen">
            <h2 class="setup-title">"Choose Your Region"</h2>

            <div class="setup-layout">
                <div class="setup-cards">
                    {cards}
                </div>

                <div class="setup-detail">
                    {move || {
                        if selected.get().is_none() {
                            view! {
                                <p class="setup-hint">
                                    "Select a region to see details."
                                </p>
                            }.into_any()
                        } else {
                            view! { <div>{preview}</div> }.into_any()
                        }
                    }}
                </div>
            </div>

            <div class="setup-actions">
                <button
                    class="btn btn-primary"
                    disabled=start_disabled
                    on:click=start_game
                >
                    "Start Game"
                </button>
            </div>
        </div>
    }
}
