use leptos::prelude::*;

use crate::state::{GameState, AFFORDABILITY_POINTS_PER_TOKEN};
use crate::types::*;

#[component]
pub fn EndGameScreen() -> impl IntoView {
    let game = use_context::<RwSignal<Option<GameState>>>().expect("game context");

    let on_play_again = move |_| {
        game.set(None);
    };

    view! {
        <div class="game-over">

            // --- Header + total score ---
            {move || game.with(|opt| {
                let gs = opt.as_ref()?;
                let stage_points = gs.total_score();
                let affordability = gs.affordability_points();
                let penalty = gs.backlash_penalty();
                Some(view! {
                    <h2 class="results-title">"Game Complete — Final Results"</h2>
                    <div class="final-score">{gs.final_score()}</div>
                    <div class="mb-8">"Total Score"</div>
                    <table class="score-table final-breakdown mb-16">
                        <tbody>
                            <tr><td>"Stage points"</td><td class="text-center">{stage_points}</td></tr>
                            <tr>
                                <td>{format!("Affordability ({} budget tokens × {})", gs.budget_tokens, AFFORDABILITY_POINTS_PER_TOKEN)}</td>
                                <td class="text-center">{format!("+{affordability}")}</td>
                            </tr>
                            <tr>
                                <td>"Social backlash on new resources"</td>
                                <td class="text-center">{format!("−{penalty}")}</td>
                            </tr>
                        </tbody>
                    </table>
                })
            })}

            // --- Per-stage score breakdown ---
            {move || game.with(|opt| {
                let gs = opt.as_ref()?;
                let n = gs.stage_history.len();
                if n == 0 { return None; }

                let mut cumulative = 0i32;
                let rows: Vec<(usize, u32, i32, i32, i32, i32)> = (0..n)
                    .map(|i| {
                        let year = gs.setup.stages[i];
                        let rel  = gs.setup.reliability_scores[i];
                        let cln  = gs.setup.clean_scores[i];
                        let sub  = rel + cln;
                        cumulative += sub;
                        (i + 1, year, rel, cln, sub, cumulative)
                    })
                    .collect();

                Some(view! {
                    <table class="score-table mt-16">
                        <thead>
                            <tr>
                                <th>"Stage"</th>
                                <th>"Year"</th>
                                <th>"Reliability Pts"</th>
                                <th>"Clean Energy Pts"</th>
                                <th>"Subtotal"</th>
                                <th>"Cumulative"</th>
                            </tr>
                        </thead>
                        <tbody>
                            {rows.into_iter().map(|(stage, year, rel, cln, sub, cum)| view! {
                                <tr>
                                    <td class="text-center">{stage}</td>
                                    <td class="text-center">{year}</td>
                                    <td class="text-center">{rel}</td>
                                    <td class="text-center">{cln}</td>
                                    <td class="text-center">{sub}</td>
                                    <td class="text-center">{cum}</td>
                                </tr>
                            }).collect_view()}
                        </tbody>
                    </table>
                })
            })}

            // --- Capacity evolution table ---
            {move || game.with(|opt| {
                let gs = opt.as_ref()?;
                let n = gs.stage_history.len();
                if n == 0 { return None; }

                let years: Vec<u32> = (0..n).map(|i| gs.setup.stages[i]).collect();

                // Build a 2-D array: rows = resources, cols = stages
                let cap_table: Vec<Vec<f64>> = (0..N_RESOURCES)
                    .map(|g| {
                        (0..n)
                            .map(|i| {
                                gs.stage_history[i]
                                    .resource_results
                                    .iter()
                                    .find(|r| r.id == g)
                                    .map(|r| r.ending_capacity_gw)
                                    .unwrap_or(0.0)
                            })
                            .collect()
                    })
                    .collect();

                Some(view! {
                    <h3 class="endgame-section-title mt-16 mb-8">
                        "Ending Capacity by Stage (GW)"
                    </h3>
                    <table class="score-table">
                        <thead>
                            <tr>
                                <th>"Resource"</th>
                                {years.into_iter().map(|y| view! { <th>{y}</th> })
                                    .collect_view()}
                            </tr>
                        </thead>
                        <tbody>
                            {cap_table.into_iter().enumerate().map(|(g, caps)| {
                                let label = RESOURCE_LABELS[g];
                                view! {
                                    <tr>
                                        <td>{label}</td>
                                        {caps.into_iter().map(|c| view! {
                                            <td class="text-center">{format!("{:.2}", c)}</td>
                                        }).collect_view()}
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                })
            })}

            // --- Play Again ---
            <div class="mt-16">
                <button class="btn btn-primary" on:click=on_play_again>
                    "Play Again"
                </button>
            </div>

        </div>
    }
}
