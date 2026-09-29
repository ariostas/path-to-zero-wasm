# Path to Zero — Rust/WASM

**Live demo:** https://ariostas.github.io/path-to-zero-wasm/

This is a Rust rewrite of [Path to Zero: The Electricity Decarbonization Game](https://github.com/PrincetonZEROLab/Path-to-Zero/), originally built in Julia using the Genie web framework and HiGHS solver.

The goal of the rewrite is to produce a **fully static website** that runs entirely in the browser with no server required, using WebAssembly for the hourly dispatch simulation.

> **Note:** This port was developed as an experiment using [Claude Code](https://claude.ai/code) (Anthropic's AI coding assistant). The AI wrote the majority of the Rust/Leptos code, guided by the original Julia implementation and human review.

## Technology stack

| Original (Julia) | This version (Rust/WASM) |
|---|---|
| Genie Framework | [Leptos](https://leptos.dev/) 0.8 (CSR) |
| JuMP + HiGHS LP | Merit-order dispatch + dynamic-programming storage schedule ([Clarabel](https://clarabel.org/) LP kept as a test reference) |
| PlotlyBase | [leptos-chartistry](https://github.com/feral-dot-io/leptos-chartistry) |
| YAML.jl | serde + serde_norway |
| CSV.jl / DataFrames.jl | csv crate (compile-time embedding) |

## Project structure

```
path-to-zero-wasm/
├── src/
│   ├── lib.rs              # WASM entry point, Leptos app root and routing
│   ├── types.rs            # Game and simulation data types
│   ├── data.rs             # Embedded CSV/YAML data, setup parsing and saving
│   ├── solver/
│   │   ├── mod.rs          # Fast hourly dispatch used by the game
│   │   └── lp.rs           # Reference LP (tests only)
│   ├── engine.rs           # Stage simulation: uncertainty, scoring, experience, backlash
│   ├── state.rs            # GameState and the game's token/scoring rules
│   └── screens/            # Setup, planning, stage results and end-game screens
├── EDG_inputs/             # CSV input data (embedded at compile time)
│   └── {2030..2050}/
├── game_setup/             # Built-in scenario YAML files
├── index.html              # Trunk entry point
├── Trunk.toml              # Trunk build config
└── style.css               # Styles
```

## Dispatch model

The original solves a linear program for each stage, which takes seconds in
WASM. This version dispatches generators in merit order and schedules the
battery with a dynamic program over its state of charge, per representative
week. With minimum-power and ramp limits relaxed, it matches the reference LP
to within about 0.3 points of clean-energy share and 0.01 points of
reliability on the game data (checked by `cargo test`), in tens of
milliseconds. It ignores the LP's minimum-power and ramp constraints.

## Saving and custom setups

After each stage, **Save Progress** downloads the game as a setup YAML file.
Loading it on the setup screen resumes the game at the next stage. Custom
setups (and saves from the original game) can be loaded the same way.

## Differences from the original

- Dispatch ignores minimum-power and ramp constraints (see above).
- The end-of-game social backlash penalty counts every new resource under
  backlash (the original only counted the first, due to an `elseif` chain).
- Clean firm becomes buildable in the stage after Innovation: Clean Firm is
  committed (the original waited an extra stage when it was bought in stage 1).

## Building

Requires Rust with the `wasm32-unknown-unknown` target and [Trunk](https://trunkrs.dev/).

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk

# Development server with hot reload
trunk serve

# Production build
trunk build --release

# Tests (release mode: the reference LP is slow in debug builds)
cargo test --release
```

The output in `dist/` is a fully static site that can be deployed anywhere (GitHub Pages, Netlify, etc.).

## License

GNU General Public License v2.0 — same as the original repository.
