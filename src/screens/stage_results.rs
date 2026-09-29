use leptos::prelude::*;
use leptos_chartistry::*;

use crate::state::GameState;
use crate::types::*;

// -----------------------------------------------------------------------
// StageResultsScreen
// -----------------------------------------------------------------------

#[component]
pub fn StageResultsScreen() -> impl IntoView {
    let game = use_context::<RwSignal<Option<GameState>>>().expect("game context");

    // Dispatch data fed into the chart; re-derived whenever game changes.
    let dispatch_data: Signal<Vec<DispatchHour>> = Signal::derive(move || {
        game.with(|opt| {
            opt.as_ref()
                .and_then(|gs| gs.last_stage_results.as_ref())
                .map(|(sr, _, _)| sr.dispatch.clone())
                .unwrap_or_default()
        })
    });

    // Stacked area series — one band per resource plus unmet demand.
    let series = Series::new(|d: &DispatchHour| d.hour as f64)
        .stack(
            Stack::new()
                .line(Line::new(|d: &DispatchHour| d.generation[0]).with_name("Natural Gas"))
                .line(Line::new(|d: &DispatchHour| d.generation[1]).with_name("Nuclear"))
                .line(Line::new(|d: &DispatchHour| d.generation[2]).with_name("Solar PV"))
                .line(Line::new(|d: &DispatchHour| d.generation[3]).with_name("Dist. Solar"))
                .line(Line::new(|d: &DispatchHour| d.generation[4]).with_name("Onshore Wind"))
                .line(Line::new(|d: &DispatchHour| d.generation[5]).with_name("Offshore Wind"))
                .line(Line::new(|d: &DispatchHour| d.generation[6]).with_name("Battery"))
                .line(Line::new(|d: &DispatchHour| d.generation[7]).with_name("Clean Firm"))
                .line(Line::new(|d: &DispatchHour| d.nonserved).with_name("Unmet Demand")),
        );

    // Demand overlay line
    let series = series
        .line(Line::new(|d: &DispatchHour| d.demand_gw).with_name("Demand"));

    // Continue action: Planning if more stages remain, EndGame otherwise.
    let on_continue = move |_| {
        game.update(|opt| {
            if let Some(gs) = opt {
                gs.continue_from_results();
            }
        });
    };

    // ---- View ----
    view! {
        <div class="results-screen">

            // --- Stage header ---
            {move || game.with(|opt| {
                let gs = opt.as_ref().unwrap();
                let completed = gs.stage_history.len();
                let year = gs.setup.stages[completed.saturating_sub(1).min(N_STAGES - 1)];
                view! {
                    <h2 class="results-title">
                        {format!("Stage {} Results — {}", completed, year)}
                    </h2>
                }
            })}

            // --- Score + reliability summary row ---
            {move || game.with(|opt| {
                let gs = opt.as_ref().unwrap();
                let (sr, _, _) = gs.last_stage_results.as_ref()?;
                let scores = sr.scores.clone();
                let nse = sr.nse_result.clone();
                let total = gs.total_score();
                Some(view! {
                    <div class="row gap-8 mt-16 mb-8">
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
                                    <tr><th colspan="2">"Grid"</th></tr>
                                </thead>
                                <tbody>
                                    <tr>
                                        <td>"Reserve margin"</td>
                                        <td class="text-center">
                                            {format!("{:.1} GW", nse.reserve_margin)}
                                        </td>
                                    </tr>
                                    <tr>
                                        <td>"Max unmet"</td>
                                        <td class="text-center">
                                            {format!("{:.1} GW", nse.max_nse_gw)}
                                        </td>
                                    </tr>
                                    <tr>
                                        <td>"Total unmet"</td>
                                        <td class="text-center">
                                            {format!("{:.0} GWh", nse.total_nse_gwh)}
                                        </td>
                                    </tr>
                                </tbody>
                            </table>
                        </div>
                        <div class="col text-center">
                            <div class="big-number results-total-score">{total}</div>
                            <div>"Cumulative score"</div>
                        </div>
                    </div>
                })
            })}

            // --- Dispatch chart ---
            <div class="chart-container">
                <Chart
                    aspect_ratio=AspectRatio::from_outer_ratio(900.0, 350.0)
                    top=RotatedLabel::middle("Hourly Dispatch (GW)")
                    left=TickLabels::aligned_floats()
                    bottom=RotatedLabel::middle("Hour")
                    right=Legend::end()
                    inner=[
                        AxisMarker::left_edge().into_inner(),
                        AxisMarker::bottom_edge().into_inner(),
                        XGuideLine::over_data().into_inner(),
                    ]
                    series=series
                    data=dispatch_data
                />
            </div>

            // --- Uncertainty narrative ---
            {move || game.with(|opt| {
                let gs = opt.as_ref()?;
                let (sr, backlash, experience) = gs.last_stage_results.as_ref()?;

                let shock_pct = sr.uncertainty.demand_shock_percent;
                let disaster = sr.uncertainty.disaster;
                let outage_week = sr.uncertainty.outage_week;
                let forced_outages = sr.uncertainty.forced_outages;
                let backlash_flags = backlash.backlash;
                let exp_rates = experience.experience_rate;

                let disaster_resources: Vec<String> = forced_outages
                    .iter()
                    .enumerate()
                    .filter(|(_, &fo)| fo)
                    .map(|(g, _)| RESOURCE_LABELS[g].to_string())
                    .collect();

                // Backlash and experience arrays are in block order.
                let block_label = |g: usize| resource_label(&gs.resource_params.names[g]).to_string();

                let backlash_resources: Vec<String> = backlash_flags
                    .iter()
                    .enumerate()
                    .filter(|(_, &b)| b)
                    .map(|(g, _)| block_label(g))
                    .collect();

                let exp_items: Vec<(String, f64)> = exp_rates
                    .iter()
                    .enumerate()
                    .filter(|(_, &r)| r > 0.0)
                    .map(|(g, &r)| (block_label(g), r * 100.0))
                    .collect();

                let shock_text = if shock_pct >= 0.0 {
                    format!("+{:.1}% higher demand than expected", shock_pct)
                } else {
                    format!("{:.1}% lower demand than expected", shock_pct)
                };

                let disaster_text = if disaster {
                    let res = if disaster_resources.is_empty() {
                        "no resources affected".to_string()
                    } else {
                        disaster_resources.join(", ")
                    };
                    Some(format!(
                        "Extreme weather event in week {} — forced outages: {}",
                        outage_week, res
                    ))
                } else {
                    None
                };

                let backlash_text = if backlash_resources.is_empty() {
                    None
                } else {
                    Some(format!(
                        "{} locked for future stages due to social backlash",
                        backlash_resources.join(", ")
                    ))
                };

                let exp_text = if exp_items.is_empty() {
                    None
                } else {
                    Some(format!(
                        "Build cost reductions: {}",
                        exp_items
                            .iter()
                            .map(|(n, r)| format!("{} \u{2212}{:.1}%", n, r))
                            .collect::<Vec<_>>()
                            .join("; ")
                    ))
                };

                Some(view! {
                    <div class="narrative-section mt-16">
                        <h3 class="narrative-title">"Stage Events"</h3>
                        <div class="narrative-item">"Demand: " {shock_text}</div>
                        {disaster_text.map(|t| view! {
                            <div class="narrative-item narrative-disaster">{t}</div>
                        })}
                        {backlash_text.map(|t| view! {
                            <div class="narrative-item narrative-backlash">{t}</div>
                        })}
                        {exp_text.map(|t| view! {
                            <div class="narrative-item narrative-experience">{t}</div>
                        })}
                    </div>
                })
            })}

            // --- Generation mix table ---
            {move || game.with(|opt| {
                let (sr, _, _) = opt.as_ref()?.last_stage_results.as_ref()?;
                let resources = sr.resource_results.clone();
                Some(view! {
                    <table class="score-table mt-16">
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
                            {resources.iter().enumerate().map(|(g, r)| {
                                let label = if g < RESOURCE_LABELS.len() {
                                    RESOURCE_LABELS[g].to_string()
                                } else {
                                    r.resource.clone()
                                };
                                let gwh = format!("{:.0}", r.gwh);
                                let pct = format!("{:.1}%", r.percent_gwh);
                                let cap = format!("{:.2}", r.ending_capacity_gw);
                                let cf  = format!("{:.1}%", r.capacity_factor);
                                view! {
                                    <tr>
                                        <td>{label}</td>
                                        <td class="text-center">{gwh}</td>
                                        <td class="text-center">{pct}</td>
                                        <td class="text-center">{cap}</td>
                                        <td class="text-center">{cf}</td>
                                    </tr>
                                }
                            }).collect_view()}
                        </tbody>
                    </table>
                })
            })}

            // --- Continue button ---
            <div class="results-actions mt-16">
                <button class="btn btn-primary" on:click=on_continue>
                    {move || game.with(|opt| {
                        let gs = opt.as_ref().unwrap();
                        if gs.stage_history.len() < N_STAGES {
                            "Continue to Next Stage"
                        } else {
                            "See Final Results"
                        }
                    })}
                </button>
            </div>

        </div>
    }
}
