# Path to Zero — Rust/WASM

This is a Rust rewrite of [Path to Zero: The Electricity Decarbonization Game](https://github.com/PrincetonZEROLab/Path-to-Zero/), originally built in Julia using the Genie web framework and HiGHS solver.

The goal of the rewrite is to produce a **fully static website** that runs entirely in the browser with no server required, using WebAssembly for the compute-intensive LP dispatch simulation.

## Technology stack

| Original (Julia) | This version (Rust/WASM) |
|---|---|
| Genie Framework | [Leptos](https://leptos.dev/) 0.8 (CSR) |
| JuMP + HiGHS | [Clarabel](https://clarabel.org/) 0.11 |
| PlotlyBase | [leptos-chartistry](https://github.com/feral-dot-io/leptos-chartistry) |
| YAML.jl | serde + serde_norway |
| CSV.jl / DataFrames.jl | csv crate (compile-time embedding) |

## Project structure

```
ptz-wasm/
├── src/
│   ├── lib.rs          # WASM entry point, Leptos app root
│   ├── types.rs        # All game and solver data types
│   ├── data.rs         # Compile-time CSV/YAML loading and parsing
│   ├── solver.rs       # LP dispatch model (Clarabel)
│   ├── engine.rs       # Game simulation logic
│   ├── setup.rs        # Game setup / state initialisation
│   └── components/     # Leptos UI components
├── EDG_inputs/         # CSV input data (embedded at compile time)
│   └── {2030..2050}/
├── game_setup/         # YAML game setup files (embedded + runtime upload)
├── index.html          # Trunk entry point
├── Trunk.toml          # Trunk build config
└── style.css           # Base styles
```

## Building

Requires Rust with the `wasm32-unknown-unknown` target and [Trunk](https://trunkrs.dev/).

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk

# Development server with hot reload
trunk serve

# Production build
trunk build --release
```

The output in `dist/` is a fully static site that can be deployed anywhere (GitHub Pages, Netlify, etc.).

## License

GNU General Public License v2.0 — same as the original repository.
