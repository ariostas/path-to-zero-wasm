use leptos::prelude::*;
use leptos_chartistry::*;

use super::planning::generation_table;
use super::{act, download_text, read, use_game, GameSignal};
use crate::data::game_setup_to_yaml;
use crate::state::GameState;
use crate::types::*;

/// Hours per chart week and weeks per year in the dispatch data.
const HOURS_PER_WEEK: usize = 168;
const WEEKS: usize = 52;

/// One point of the dispatch chart (GW), `x` in days since the year start.
#[derive(Debug, Clone, Default)]
struct ChartPoint {
    day: f64,
    generation: [f64; N_RESOURCES],
    /// Charging power as a negative number.
    battery_charge: f64,
    nonserved: f64,
    demand: f64,
}

impl ChartPoint {
    fn from_hour(d: &DispatchHour) -> Self {
        Self {
            day: d.hour as f64 / 24.0,
            generation: d.generation,
            battery_charge: d.storage_charge,
            nonserved: d.nonserved,
            demand: d.demand_gw,
        }
    }

    /// Average of a run of hours, plotted at the start of the run.
    fn average(hours: &[DispatchHour]) -> Self {
        let n = hours.len().max(1) as f64;
        let mut p = Self { day: hours.first().map_or(0.0, |h| h.hour as f64 / 24.0), ..Self::default() };
        for h in hours {
            for (acc, g) in p.generation.iter_mut().zip(h.generation) {
                *acc += g / n;
            }
            p.battery_charge += h.storage_charge / n;
            p.nonserved += h.nonserved / n;
            p.demand += h.demand_gw / n;
        }
        p
    }
}

/// Chart points for `week` (1-based), or daily averages for the full year
/// when `week == 0`.
fn chart_points(dispatch: &[DispatchHour], week: usize) -> Vec<ChartPoint> {
    if week == 0 {
        dispatch.chunks(24).map(ChartPoint::average).collect()
    } else {
        let start = ((week - 1) * HOURS_PER_WEEK).min(dispatch.len());
        let end = (start + HOURS_PER_WEEK).min(dispatch.len());
        dispatch[start..end].iter().map(ChartPoint::from_hour).collect()
    }
}

/// Stack order (bottom to top) and colours, matching the original game.
const STACK: [(&str, Colour); N_RESOURCES] = [
    ("nuclear", Colour::from_rgb(0x8c, 0x56, 0x4b)),
    ("clean_firm", Colour::from_rgb(0x17, 0xbe, 0xcf)),
    ("onshore_wind", Colour::from_rgb(0x2c, 0xa0, 0x2c)),
    ("offshore_wind", Colour::from_rgb(0x1f, 0x77, 0xb4)),
    ("solar_pv", Colour::from_rgb(0xeb, 0xc3, 0x34)),
    ("distributed_solar", Colour::from_rgb(0xff, 0x7f, 0x0e)),
    ("battery", Colour::from_rgb(0xe3, 0x77, 0xc2)),
    ("natural_gas", Colour::from_rgb(0xd6, 0x27, 0x28)),
];

fn dispatch_series(is_wy_setup: bool) -> Series<ChartPoint, f64, f64> {
    let mut stack = Stack::new();
    for (name, colour) in STACK {
        let g = resource_index(name).expect("known resource");
        stack = stack.line(
            Line::new(move |p: &ChartPoint| p.generation[g])
                .with_name(region_resource_label(name, is_wy_setup))
                .with_colour(colour),
        );
    }
    stack = stack.line(
        Line::new(|p: &ChartPoint| p.nonserved)
            .with_name("Unmet Demand")
            .with_colour(Colour::from_rgb(0, 0, 0)),
    );

    Series::new(|p: &ChartPoint| p.day)
        .stack(stack)
        .line(
            Line::new(|p: &ChartPoint| p.battery_charge)
                .with_name("Battery Charging")
                .with_colour(Colour::from_rgb(0x94, 0x67, 0xbd)),
        )
        .line(
            Line::new(|p: &ChartPoint| p.demand)
                .with_name("Demand")
                .with_colour(Colour::from_rgb(0x55, 0x55, 0x55))
                .with_width(2.0),
        )
}

// -----------------------------------------------------------------------
// StageResultsScreen
// -----------------------------------------------------------------------

#[component]
pub fn StageResultsScreen() -> impl IntoView {
    let game = use_game();

    // 0 = full year (daily averages); 1..=52 = a single week, hourly.
    let chart_week = RwSignal::new(0usize);

    let chart_data: Signal<Vec<ChartPoint>> = Signal::derive(move || {
        let week = chart_week.get();
        game.with(|opt| {
            let (sr, _, _) = opt.as_ref()?.last_stage_results.as_ref()?;
            Some(chart_points(&sr.dispatch, week))
        })
        .unwrap_or_default()
    });

    let week_options = std::iter::once(view! { <option value="0">"Full year (daily average)"</option> }.into_any())
        .chain((1..=WEEKS).map(|w| {
            view! { <option value=w.to_string()>{format!("Week {w} (hourly)")}</option> }.into_any()
        }))
        .collect_view();

    // ---- View ----
    view! {
        <div class="results-screen">

            // --- Stage header ---
            {move || read(game, |gs| {
                let completed = gs.current_stage_num() - 1;
                let year = gs.setup.stages[completed.clamp(1, N_STAGES) - 1];
                view! {
                    <h2 class="results-title">
                        {format!("Stage {} Results — {}", completed, year)}
                    </h2>
                }
            })}

            // --- Score + reliability summary row ---
            {move || game.with(|opt| {
                let gs = opt.as_ref()?;
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
            <div class="chart-controls">
                <label>
                    "Show: "
                    <select
                        prop:value=move || chart_week.get().to_string()
                        on:change=move |ev| chart_week.set(event_target_value(&ev).parse().unwrap_or(0))
                    >
                        {week_options}
                    </select>
                </label>
            </div>
            <div class="chart-container">
                <Chart
                    aspect_ratio=AspectRatio::from_outer_ratio(900.0, 350.0)
                    top=RotatedLabel::middle("Dispatch (GW)")
                    left=TickLabels::aligned_floats()
                    bottom=vec![TickLabels::aligned_floats().into_edge(), RotatedLabel::middle("Day of year").into_edge()]
                    right=Legend::end()
                    inner=[
                        AxisMarker::left_edge().into_inner(),
                        AxisMarker::bottom_edge().into_inner(),
                        XGuideLine::over_data().into_inner(),
                    ]
                    tooltip=Tooltip::left_cursor()
                    series=dispatch_series(read(game, |gs| gs.setup.is_wy_setup).unwrap_or(false))
                    data=chart_data
                />
            </div>

            // --- Uncertainty narrative ---
            {move || game.with(|opt| {
                let gs = opt.as_ref()?;
                let (sr, backlash, experience) = gs.last_stage_results.as_ref()?;

                let shock_pct = sr.uncertainty.demand_shock_percent;
                let disaster = sr.uncertainty.disaster;
                let outage_week = sr.uncertainty.outage_week;

                // Forced outages are in CSV order.
                let disaster_resources: Vec<String> = sr.uncertainty.forced_outages
                    .iter()
                    .enumerate()
                    .filter(|(_, &fo)| fo)
                    .map(|(g, _)| gs.label(RESOURCE_ORDER[g]).to_string())
                    .collect();

                // Backlash and experience arrays are in block order.
                let block_label = |g: usize| gs.label(&gs.resource_params.names[g]).to_string();

                let backlash_resources: Vec<String> = backlash.backlash
                    .iter()
                    .enumerate()
                    .filter(|(_, &b)| b)
                    .map(|(g, _)| block_label(g))
                    .collect();

                let exp_items: Vec<(String, f64)> = experience.experience_rate
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

                let disaster_text = disaster.then(|| {
                    let res = if disaster_resources.is_empty() {
                        "no resources affected".to_string()
                    } else {
                        disaster_resources.join(", ")
                    };
                    format!(
                        "Extreme weather in weeks {}–{} — forced outages: {}",
                        outage_week,
                        outage_week + 3,
                        res
                    )
                });

                let backlash_text = (!backlash_resources.is_empty()).then(|| {
                    format!(
                        "{} locked for the next stage due to social backlash",
                        backlash_resources.join(", ")
                    )
                });

                let exp_text = (!exp_items.is_empty()).then(|| {
                    format!(
                        "Experience gains (GW per build token grows by this per token spent): {}",
                        exp_items
                            .iter()
                            .map(|(n, r)| format!("{} +{:.1}%", n, r))
                            .collect::<Vec<_>>()
                            .join("; ")
                    )
                });

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
                let gs = opt.as_ref()?;
                let (sr, _, _) = gs.last_stage_results.as_ref()?;
                Some(view! { <div class="mt-16">{generation_table(&sr.resource_results, gs.setup.is_wy_setup)}</div> })
            })}

            // --- Continue button ---
            <div class="results-actions mt-16">
                <button class="btn btn-primary" on:click=move |_| act(game, GameState::continue_from_results)>
                    {move || read(game, |gs| {
                        if gs.is_game_over() { "See Final Results" } else { "Continue to Next Stage" }
                    })}
                </button>
                <Show when=move || read(game, |gs| !gs.is_game_over()).unwrap_or(false)>
                    <button class="btn btn-secondary" on:click=move |_| save_progress(game)>
                        "Save Progress"
                    </button>
                </Show>
            </div>

        </div>
    }
}

/// Download the game state at the start of the next stage as a setup file
/// that can be loaded from the setup screen to resume.
fn save_progress(game: GameSignal) {
    let Some((stage, yaml)) = read(game, |gs| (gs.current_stage_num(), game_setup_to_yaml(&gs.save_setup())))
    else {
        return;
    };
    if let Err(e) = download_text(&format!("path_to_zero_stage_{stage}.yml"), &yaml) {
        web_sys::console::error_1(&e);
    }
}
