use leptos::prelude::*;

use super::{act, read, use_game, GameSignal};
use crate::state::*;
use crate::types::*;

#[component]
pub fn PlanningScreen() -> impl IntoView {
    let game = use_game();
    let confirming = RwSignal::new(false);

    let resource_cards: Vec<_> = (0..N_RESOURCES).map(|g| resource_card(game, g)).collect();

    let shaping_buttons: Vec<_> = ShapingKind::ALL
        .into_iter()
        .map(|kind| {
            view! {
                <div class="shaping-item">
                    <button
                        class="shaping-btn"
                        class:active=move || read(game, |gs| gs.shaping_token_state.get(kind)).unwrap_or(false)
                        disabled=move || !read(game, |gs| gs.can_toggle_shaping(kind)).unwrap_or(false)
                        on:click=move |_| act(game, |gs| gs.toggle_shaping(kind))
                    >
                        {kind.label()}
                    </button>
                    <div class="shaping-desc">
                        {kind.description()}
                        {move || read(game, |gs| gs.shaping_locked.get(kind)).unwrap_or(false)
                            .then_some(" (committed)")}
                    </div>
                </div>
            }
        })
        .collect();

    view! {
        <div class="planning-screen">

            // --- Stage progress bar ---
            <div class="planning-stage-bar">
                {move || read(game, |gs| {
                    let current = gs.current_stage_num();
                    gs.setup.stages.iter().enumerate().map(|(i, &year)| {
                        view! {
                            <div
                                class="stage-btn"
                                class:active=i + 1 == current
                                class:done=i + 1 < current
                            >
                                {year.to_string()}
                            </div>
                        }
                    }).collect_view()
                })}
            </div>

            // --- Token budget + score summary ---
            <div class="planning-summary mt-8">
                <span>
                    <strong>"Build tokens: "</strong>
                    {move || read(game, |gs| format!(
                        "{} remaining of {}",
                        gs.build_tokens_remaining(),
                        gs.build_token_budget()
                    ))}
                </span>
                <span>
                    <strong>"Budget tokens: "</strong>
                    {move || read(game, |gs| format!(
                        "{} (worth {} pts at the end)",
                        gs.budget_tokens,
                        gs.affordability_points()
                    ))}
                </span>
                <span>
                    <strong>"Shaping tokens: "</strong>
                    {move || read(game, |gs| gs.shaping_tokens_available)}
                    " available"
                </span>
                <span>
                    <strong>"Cumulative score: "</strong>
                    {move || read(game, |gs| gs.total_score())}
                    " pts"
                </span>
            </div>

            // --- Budget token market ---
            <div class="token-market mt-8">
                {market_button(game, "Buy 2 build tokens (−1 budget)", GameState::can_buy_build_tokens, GameState::buy_build_tokens)}
                {market_button(game, "Undo", GameState::can_undo_buy_build_tokens, GameState::undo_buy_build_tokens)}
                {market_button(game, "Sell 3 build tokens (+1 budget)", GameState::can_sell_build_tokens, GameState::sell_build_tokens)}
                {market_button(game, "Buy shaping token (−1 budget)", GameState::can_buy_shaping_token, GameState::buy_shaping_token)}
                {market_button(game, "Undo", GameState::can_undo_buy_shaping_token, GameState::undo_buy_shaping_token)}
            </div>

            // --- Resource block cards ---
            <div class="row flex-wrap mt-16">
                {resource_cards}
            </div>

            // --- Shaping tokens ---
            <div class="planning-shaping mt-16">
                <div class="bold mb-8">"Shaping Tokens"</div>
                <div class="flex flex-wrap">{shaping_buttons}</div>
            </div>

            // --- Action buttons ---
            <div class="planning-actions mt-16">
                <button class="btn btn-secondary" on:click=move |_| act(game, GameState::run_preview)>
                    "Preview"
                </button>
                <button class="btn btn-danger" on:click=move |_| confirming.set(true)>
                    "Advance Stage"
                </button>
            </div>

            // --- Advance confirmation ---
            <Show when=move || confirming.get()>
                <div class="modal-overlay">
                    <div class="modal-card">
                        <p class="bold mb-8">"Advance to the next stage?"</p>
                        <p class="mb-16">
                            {move || read(game, |gs| {
                                let left = gs.build_tokens_remaining();
                                if left > 0 {
                                    format!("You still have {left} unallocated build token(s); they will be lost.")
                                } else {
                                    "Your plan will be locked in and simulated with uncertainty.".to_string()
                                }
                            })}
                        </p>
                        <div class="planning-actions">
                            <button class="btn btn-secondary" on:click=move |_| confirming.set(false)>
                                "Cancel"
                            </button>
                            <button
                                class="btn btn-danger"
                                on:click=move |_| {
                                    confirming.set(false);
                                    act(game, GameState::advance);
                                }
                            >
                                "Advance"
                            </button>
                        </div>
                    </div>
                </div>
            </Show>

            // --- Preview results (shown after clicking Preview) ---
            {move || game.with(|opt| {
                let gs = opt.as_ref()?;
                gs.preview.as_ref().map(|p| preview_panel(p, gs.setup.is_wy_setup))
            })}

        </div>
    }
}

/// A budget-token market button.
fn market_button(
    game: GameSignal,
    label: &'static str,
    enabled: fn(&GameState) -> bool,
    action: fn(&mut GameState),
) -> impl IntoView {
    view! {
        <button
            class="btn btn-sm btn-primary"
            disabled=move || !read(game, enabled).unwrap_or(false)
            on:click=move |_| act(game, action)
        >
            {label}
        </button>
    }
}

/// Card for resource block `g` with its build-token controls.
fn resource_card(game: GameSignal, g: usize) -> impl IntoView {
    let status = move || {
        read(game, |gs| {
            if gs.is_locked(g) {
                Some("Locked this stage: social backlash")
            } else if gs.is_clean_firm_unavailable(g) {
                Some("Requires a committed Innovation: Clean Firm token")
            } else if gs.resource_params.build_cost[g] <= 0.0 {
                Some("Not available in this region")
            } else {
                None
            }
        })
        .flatten()
    };

    view! {
        <div
            class="resource-block"
            class:existing=move || read(game, |gs| gs.is_existing(g)).unwrap_or(false)
            class:locked=move || status().is_some()
        >
            <div class="rb-name">
                {move || read(game, |gs| gs.setup.resource_blocks[g].name.clone())}
            </div>
            <div class="rb-cap">
                {move || read(game, |gs| {
                    let planned = gs.planned_capacity(g);
                    if gs.is_existing(g) {
                        format!("{:.1} of {:.1} GW retained", planned, gs.resource_params.start_capacity[g])
                    } else {
                        format!("{planned:.1} GW")
                    }
                })}
            </div>
            <div class="rb-info">
                {move || read(game, |gs| {
                    let cost = gs.resource_params.build_cost[g];
                    let risk = format!("{:?}", gs.setup.resource_blocks[g].backlash_risk).to_lowercase();
                    if gs.is_existing(g) {
                        format!("retain {cost:.1} GW/token · backlash risk: {risk}")
                    } else {
                        format!("+{cost:.1} GW/token · backlash risk: {risk}")
                    }
                })}
            </div>
            <div class="rb-info">
                {move || read(game, |gs| gs.setup.resource_blocks[g].edg_data_info.clone())}
            </div>
            {move || status().map(|s| view! { <div class="rb-status">{s}</div> })}
            <div class="rb-token-controls">
                <button
                    class="btn btn-sm btn-secondary btn-token"
                    disabled=move || !read(game, |gs| gs.can_remove_token(g)).unwrap_or(false)
                    on:click=move |_| act(game, |gs| gs.remove_token(g))
                >"-"</button>
                <span class="rb-token-count">
                    {move || read(game, |gs| gs.resource_params.build_tokens[g])}
                </span>
                <button
                    class="btn btn-sm btn-primary btn-token"
                    disabled=move || !read(game, |gs| gs.can_add_token(g)).unwrap_or(false)
                    on:click=move |_| act(game, |gs| gs.add_token(g))
                >"+"</button>
            </div>
            <div class="rb-token-boxes">
                {move || read(game, |gs| {
                    let budget = gs.build_token_budget().max(0);
                    let n = gs.resource_params.build_tokens[g];
                    (0..budget)
                        .map(|i| view! { <span class="token-box-sm" class:active=i < n></span> })
                        .collect_view()
                })}
            </div>
        </div>
    }
}

fn preview_panel(p: &SimulationResults, is_wy_setup: bool) -> impl IntoView {
    let scores = p.scores.clone();
    let resources = p.resource_results.clone();
    let nse = p.nse_result.clone();
    view! {
        <div class="preview-panel mt-16">
            <h3 class="bold mb-8">"Preview Results (no uncertainty)"</h3>

            <div class="row gap-8 mb-16">
                <div class="col">
                    <table class="score-table">
                        <thead>
                            <tr><th>"Reliability"</th><th>"Points"</th></tr>
                        </thead>
                        <tbody>
                            <tr>
                                <td>{format!("{:.2}%", scores.reliability)}</td>
                                <td class="text-center">{scores.reliability_points}</td>
                            </tr>
                        </tbody>
                    </table>
                </div>
                <div class="col">
                    <table class="score-table clean">
                        <thead>
                            <tr><th>"Clean Energy"</th><th>"Points"</th></tr>
                        </thead>
                        <tbody>
                            <tr>
                                <td>{format!("{:.1}%", scores.clean_share)}</td>
                                <td class="text-center">{scores.clean_points}</td>
                            </tr>
                        </tbody>
                    </table>
                </div>
                <div class="col">
                    <table class="score-table">
                        <thead>
                            <tr><th colspan="2">"Grid Summary"</th></tr>
                        </thead>
                        <tbody>
                            <tr>
                                <td>"Reserve margin"</td>
                                <td class="text-center">{format!("{:.1} GW", nse.reserve_margin)}</td>
                            </tr>
                            <tr>
                                <td>"Max NSE"</td>
                                <td class="text-center">{format!("{:.1} GW", nse.max_nse_gw)}</td>
                            </tr>
                        </tbody>
                    </table>
                </div>
            </div>

            {generation_table(&resources, is_wy_setup)}
        </div>
    }
}

/// Generation mix table, shared by the preview and stage results.
pub(crate) fn generation_table(resources: &[ResourceResult], is_wy_setup: bool) -> impl IntoView {
    let rows = resources
        .iter()
        .map(|r| {
            view! {
                <tr>
                    <td>{region_resource_label(&r.resource, is_wy_setup).to_string()}</td>
                    <td class="text-center">{format!("{:.0}", r.gwh)}</td>
                    <td class="text-center">{format!("{:.1}%", r.percent_gwh)}</td>
                    <td class="text-center">{format!("{:.2}", r.ending_capacity_gw)}</td>
                    <td class="text-center">{format!("{:.1}%", r.capacity_factor)}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        <div class="table-scroll">
            <table class="score-table">
                <thead>
                    <tr>
                        <th>"Resource"</th>
                        <th>"GWh"</th>
                        <th>"% gen"</th>
                        <th>"Capacity (GW)"</th>
                        <th>"Cap. factor"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        </div>
    }
}
