use leptos::prelude::*;

use crate::engine;
use crate::state::GameState;
use crate::types::*;

#[component]
pub fn PlanningScreen() -> impl IntoView {
    let game = use_context::<RwSignal<Option<GameState>>>().expect("game context");

    // -----------------------------------------------------------------------
    // Per-resource build token controls (8 cards, one per resource)
    // -----------------------------------------------------------------------
    let resource_cards: Vec<_> = (0..N_RESOURCES)
        .map(|g| {
            let add = move |_| {
                game.update(|opt| {
                    let gs = opt.as_mut().unwrap();
                    if gs.build_tokens_remaining() > 0 && !gs.is_locked(g) {
                        gs.resource_params.build_tokens[g] += 1;
                        gs.preview = None;
                    }
                });
            };
            let remove = move |_| {
                game.update(|opt| {
                    let gs = opt.as_mut().unwrap();
                    if gs.resource_params.build_tokens[g] > 0 && !gs.is_locked(g) {
                        gs.resource_params.build_tokens[g] -= 1;
                        gs.preview = None;
                    }
                });
            };

            view! {
                <div
                    class="resource-block"
                    class:existing=move || game.with(|opt| {
                        !opt.as_ref().unwrap().setup.resource_blocks[g].new_resource
                    })
                    class:locked=move || game.with(|opt| opt.as_ref().unwrap().is_locked(g))
                >
                    <div class="rb-name">
                        {move || game.with(|opt| {
                            opt.as_ref().unwrap().setup.resource_blocks[g].name.clone()
                        })}
                    </div>
                    <div class="rb-cap">
                        {move || game.with(|opt| {
                            let gs = opt.as_ref().unwrap();
                            let start = gs.resource_params.start_capacity[g];
                            let cost = gs.resource_params.build_cost[g];
                            let tokens = gs.resource_params.build_tokens[g];
                            format!("{:.1} GW", start + cost * tokens as f64)
                        })}
                    </div>
                    <div class="rb-info">
                        {move || game.with(|opt| {
                            let gs = opt.as_ref().unwrap();
                            let cost = gs.resource_params.build_cost[g];
                            let is_new = gs.setup.resource_blocks[g].new_resource;
                            if is_new {
                                format!("+{:.1} GW/token", cost)
                            } else {
                                format!("retain {:.1} GW/token", cost)
                            }
                        })}
                    </div>
                    <div class="rb-token-controls">
                        <button
                            class="btn btn-sm btn-secondary btn-token"
                            disabled=move || game.with(|opt| {
                                let gs = opt.as_ref().unwrap();
                                gs.resource_params.build_tokens[g] == 0 || gs.is_locked(g)
                            })
                            on:click=remove
                        >"-"</button>
                        <span class="rb-token-count">
                            {move || game.with(|opt| {
                                opt.as_ref().unwrap().resource_params.build_tokens[g]
                            })}
                        </span>
                        <button
                            class="btn btn-sm btn-primary btn-token"
                            disabled=move || game.with(|opt| {
                                let gs = opt.as_ref().unwrap();
                                gs.build_tokens_remaining() == 0 || gs.is_locked(g)
                            })
                            on:click=add
                        >"+"</button>
                    </div>
                    <div class="rb-token-boxes">
                        {move || game.with(|opt| {
                            let gs = opt.as_ref().unwrap();
                            let budget = gs.build_token_budget() as usize;
                            let n = gs.resource_params.build_tokens[g] as usize;
                            (0..budget)
                                .map(|i| {
                                    let active = i < n;
                                    view! {
                                        <span class="token-box-sm" class:active=active></span>
                                    }
                                })
                                .collect_view()
                        })}
                    </div>
                </div>
            }
        })
        .collect();

    // -----------------------------------------------------------------------
    // Preview action (no uncertainty, shows projected scores)
    // -----------------------------------------------------------------------
    let run_preview = move |_| {
        game.update(|opt| {
            if let Some(gs) = opt {
                let results = engine::run_simulation(
                    gs.current_year(),
                    &gs.resource_params,
                    &gs.setup.scoring_params,
                    &gs.shaping_tokens(),
                    gs.current_stage_num(),
                    gs.is_new_nuclear(),
                );
                gs.preview = Some(results);
            }
        });
    };

    // -----------------------------------------------------------------------
    // Advance stage (applies uncertainty, transitions to StageResults)
    // -----------------------------------------------------------------------
    let advance = move |_| {
        game.update(|opt| {
            if let Some(gs) = opt {
                let (stage_results, backlash, experience) = engine::advance_stage(
                    gs.current_stage_num(),
                    gs.current_year(),
                    &gs.resource_params,
                    &gs.shaping_tokens(),
                    &gs.setup.uncertainty_params,
                    &gs.setup.scoring_params,
                    gs.setup.experience_rate,
                    &gs.setup.resource_blocks,
                    &gs.setup.backlash_rates,
                    gs.setup.is_wy_setup,
                    gs.is_new_nuclear(),
                );
                gs.apply_stage_results(stage_results, backlash, experience);
            }
        });
    };

    // -----------------------------------------------------------------------
    // View
    // -----------------------------------------------------------------------
    view! {
        <div class="planning-screen">

            // --- Stage progress bar ---
            <div class="planning-stage-bar">
                {move || game.with(|opt| {
                    let gs = opt.as_ref().unwrap();
                    let current = gs.current_stage_num();
                    gs.setup.stages.iter().enumerate().map(|(i, &year)| {
                        let is_active = i + 1 == current;
                        let is_done = i + 1 < current;
                        view! {
                            <div
                                class="stage-btn"
                                class:active=is_active
                                class:done=is_done
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
                    {move || game.with(|opt| {
                        let gs = opt.as_ref().unwrap();
                        format!(
                            "{} remaining of {}",
                            gs.build_tokens_remaining(),
                            gs.build_token_budget()
                        )
                    })}
                </span>
                <span>
                    <strong>"Cumulative score: "</strong>
                    {move || game.with(|opt| opt.as_ref().unwrap().total_score())}
                    " pts"
                </span>
            </div>

            // --- Resource block cards ---
            <div class="row flex-wrap mt-16">
                {resource_cards}
            </div>

            // --- Shaping tokens ---
            <div class="planning-shaping mt-16">
                <div class="bold mb-8">"Shaping Tokens"</div>
                <div class="flex flex-wrap">
                    <button
                        class="shaping-btn"
                        class:active=move || game.with(|opt| {
                            opt.as_ref().unwrap().shaping_token_state.resilience
                        })
                        on:click=move |_| game.update(|opt| {
                            let gs = opt.as_mut().unwrap();
                            gs.shaping_token_state.resilience = !gs.shaping_token_state.resilience;
                            gs.preview = None;
                        })
                    >"Resilience"</button>

                    <button
                        class="shaping-btn"
                        class:active=move || game.with(|opt| {
                            opt.as_ref().unwrap().shaping_token_state.innovation_experience
                        })
                        on:click=move |_| game.update(|opt| {
                            let gs = opt.as_mut().unwrap();
                            gs.shaping_token_state.innovation_experience =
                                !gs.shaping_token_state.innovation_experience;
                            gs.preview = None;
                        })
                    >"Innovation: Experience"</button>

                    <button
                        class="shaping-btn"
                        class:active=move || game.with(|opt| {
                            opt.as_ref().unwrap().shaping_token_state.innovation_clean_firm
                        })
                        on:click=move |_| game.update(|opt| {
                            let gs = opt.as_mut().unwrap();
                            gs.shaping_token_state.innovation_clean_firm =
                                !gs.shaping_token_state.innovation_clean_firm;
                            gs.preview = None;
                        })
                    >"Innovation: Clean Firm"</button>

                    <button
                        class="shaping-btn"
                        class:active=move || game.with(|opt| {
                            opt.as_ref().unwrap().shaping_token_state.social_license
                        })
                        on:click=move |_| game.update(|opt| {
                            let gs = opt.as_mut().unwrap();
                            gs.shaping_token_state.social_license =
                                !gs.shaping_token_state.social_license;
                            gs.preview = None;
                        })
                    >"Social License"</button>
                </div>
            </div>

            // --- Action buttons ---
            <div class="planning-actions mt-16">
                <button class="btn btn-secondary" on:click=run_preview>
                    "Preview"
                </button>
                <button class="btn btn-danger" on:click=advance>
                    "Advance Stage"
                </button>
            </div>

            // --- Preview results (shown after clicking Preview) ---
            {move || game.with(|opt| {
                opt.as_ref().unwrap().preview.as_ref().map(|p| {
                    let scores = p.scores.clone();
                    let resources = p.resource_results.clone();
                    let nse = p.nse_result.clone();
                    view! {
                        <div class="preview-panel mt-16">
                            <h3 class="bold mb-8">"Preview Results"</h3>

                            <div class="row gap-8 mb-16">
                                // Reliability score
                                <div class="col">
                                    <table class="score-table">
                                        <thead>
                                            <tr>
                                                <th>"Reliability"</th>
                                                <th>"Points"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            <tr>
                                                <td>{format!("{:.2}%", scores.reliability)}</td>
                                                <td class="text-center">{scores.reliability_points}</td>
                                            </tr>
                                        </tbody>
                                    </table>
                                </div>
                                // Clean energy score
                                <div class="col">
                                    <table class="score-table clean">
                                        <thead>
                                            <tr>
                                                <th>"Clean Energy"</th>
                                                <th>"Points"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            <tr>
                                                <td>{format!("{:.1}%", scores.clean_share)}</td>
                                                <td class="text-center">{scores.clean_points}</td>
                                            </tr>
                                        </tbody>
                                    </table>
                                </div>
                                // NSE summary
                                <div class="col">
                                    <table class="score-table">
                                        <thead>
                                            <tr>
                                                <th colspan="2">"Grid Summary"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            <tr>
                                                <td>"Reserve margin"</td>
                                                <td class="text-center">
                                                    {format!("{:.1} GW", nse.reserve_margin)}
                                                </td>
                                            </tr>
                                            <tr>
                                                <td>"Max NSE"</td>
                                                <td class="text-center">
                                                    {format!("{:.1} GW", nse.max_nse_gw)}
                                                </td>
                                            </tr>
                                        </tbody>
                                    </table>
                                </div>
                            </div>

                            // Generation mix table
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
                                <tbody>
                                    {resources.iter().map(|r| {
                                        let name = r.resource.clone();
                                        let gwh = format!("{:.0}", r.gwh);
                                        let pct = format!("{:.1}%", r.percent_gwh);
                                        let cap = format!("{:.2}", r.ending_capacity_gw);
                                        let cf = format!("{:.1}%", r.capacity_factor);
                                        view! {
                                            <tr>
                                                <td>{name}</td>
                                                <td class="text-center">{gwh}</td>
                                                <td class="text-center">{pct}</td>
                                                <td class="text-center">{cap}</td>
                                                <td class="text-center">{cf}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }
                })
            })}

        </div>
    }
}
