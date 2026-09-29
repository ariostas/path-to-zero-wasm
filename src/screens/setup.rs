use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::use_game;
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

/// A selectable setup: display name, short code, and parsed setup.
#[derive(Clone)]
struct SetupEntry {
    name: String,
    code: String,
    setup: GameSetup,
}

/// Read the text of the file chosen in a file `<input>`.
async fn read_selected_file(input: web_sys::HtmlInputElement) -> Result<(String, String), String> {
    let file = input.files().and_then(|f| f.get(0)).ok_or("No file selected")?;
    let text = wasm_bindgen_futures::JsFuture::from(file.text())
        .await
        .map_err(|_| "Could not read the file".to_string())?;
    Ok((file.name(), text.as_string().unwrap_or_default()))
}

// ---------------------------------------------------------------------------
// SetupScreen component
// ---------------------------------------------------------------------------

#[component]
pub fn SetupScreen() -> impl IntoView {
    let game = use_game();

    let selected: RwSignal<Option<usize>> = RwSignal::new(None);
    let upload_error: RwSignal<Option<String>> = RwSignal::new(None);

    // Built-in setups are embedded and known to be valid; uploads are appended.
    let setups: RwSignal<Vec<SetupEntry>> = RwSignal::new(
        builtin_setups()
            .into_iter()
            .map(|(filename, yaml)| SetupEntry {
                name: display_name(filename).to_string(),
                code: short_code(filename).to_string(),
                setup: parse_game_setup(yaml).expect("built-in setups are valid"),
            })
            .collect(),
    );

    let on_upload = move |ev: leptos::ev::Event| {
        let Some(input) = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        leptos::task::spawn_local(async move {
            let result = read_selected_file(input.clone())
                .await
                .and_then(|(name, text)| parse_game_setup(&text).map(|setup| (name, setup)));
            // Allow re-selecting the same file after fixing it.
            input.set_value("");
            match result {
                Ok((name, setup)) => {
                    upload_error.set(None);
                    setups.update(|s| {
                        s.push(SetupEntry { name, code: "FILE".to_string(), setup });
                        selected.set(Some(s.len() - 1));
                    });
                }
                Err(e) => upload_error.set(Some(e)),
            }
        });
    };

    // ---- Setup cards ----
    let cards = move || {
        setups.with(|s| {
            s.iter()
                .enumerate()
                .map(|(i, entry)| {
                    let setup = &entry.setup;
                    let year_range = format!("{} – {}", setup.stages[0], setup.stages[setup.stages.len() - 1]);
                    let meta = if setup.current_stage > 1 {
                        format!("Saved game: stage {}", setup.current_stage)
                    } else {
                        let total_tokens: i32 = setup.available_build_tokens.iter().sum();
                        format!("{total_tokens} build tokens")
                    };
                    view! {
                        <button
                            class="setup-card"
                            class:selected=move || selected.get() == Some(i)
                            on:click=move |_| selected.set(Some(i))
                        >
                            <span class="setup-card-code">{entry.code.clone()}</span>
                            <span class="setup-card-name">{entry.name.clone()}</span>
                            <span class="setup-card-meta">{year_range}</span>
                            <span class="setup-card-meta">{meta}</span>
                        </button>
                    }
                })
                .collect_view()
        })
    };

    // ---- Preview panel (reactive on selection) ----
    let preview = move || {
        let idx = selected.get()?;
        setups.with(|s| {
            let SetupEntry { name, setup, .. } = s.get(idx)?;
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

            let resume = (setup.current_stage > 1).then(|| {
                format!(
                    "Resumes at stage {} ({}) with {} points so far.",
                    setup.current_stage,
                    setup.stages[setup.current_stage - 1],
                    setup.reliability_scores.iter().sum::<i32>() + setup.clean_scores.iter().sum::<i32>()
                )
            });
            let shaping = setup.current_stage_shaping_tokens;
            let budget = setup.available_budget_tokens;

            Some(view! {
                <div class="setup-preview">
                    <h3>{name}</h3>
                    {resume.map(|r| view! { <p class="mb-8">{r}</p> })}

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
                        <strong>"Shaping tokens: "</strong>
                        {shaping}
                    </p>
                    <p>
                        <strong>"Budget tokens: "</strong>
                        {budget}
                    </p>
                </div>
            })
        })
    };

    // ---- Start game action ----
    let start_game = move |_| {
        let setup = selected.get().and_then(|idx| setups.with(|s| s.get(idx).map(|e| e.setup.clone())));
        if let Some(setup) = setup {
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

            <div class="setup-upload">
                <label>
                    "Or load a custom setup or saved game (.yml): "
                    <input type="file" accept=".yml,.yaml" on:change=on_upload />
                </label>
                {move || upload_error.get().map(|e| view! { <p class="upload-error">{e}</p> })}
            </div>

            <div class="setup-actions">
                <button
                    class="btn btn-primary"
                    disabled=move || selected.get().is_none()
                    on:click=start_game
                >
                    "Start Game"
                </button>
            </div>
        </div>
    }
}
