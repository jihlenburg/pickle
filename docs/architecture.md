# Architecture

## Overview

pickle is a Tauri desktop app with a static frontend and a Rust backend.

- The frontend is plain HTML/CSS/JS with no bundler or framework.
- The backend owns device discovery, DFP extraction, settings persistence, code generation, and native dialogs.
- All cross-boundary communication happens through Tauri `invoke()` commands and `menu-action` / `verify-progress` events.

## Project Layout

```text
pickle/
  frontend/
    index.html
    static/
      app/
        config.js            Unified theme tokens, typography, and shell-level UI constants
        model.js             Pure frontend state and normalization helpers
        00-05*.js            Core state, rendering, interactions, workflow modules, and editors
        06-shell.js          Shell event wiring, catalog freshness, and theme handling
        07-verification.js   Datasheet verification flow and overlay application
        08-bootstrap.js      Final startup orchestration, menu events, and tooltips
      style.css              Import manifest for the split stylesheet modules
      styles/                Foundation, component, verification, shell, and responsive CSS modules
      pin_descriptions.js    Pattern-based descriptions and grouping helpers
  src-tauri/
    src/
      main.rs                Tauri bootstrap, plugins, native menu wiring
      commands.rs            Shared IPC request/response types plus command helpers
      commands/              Concern-focused Tauri command implementations
      settings.rs            TOML-backed appearance/startup settings
      codegen/
        generate.rs          Configurable `mcu_init.c/.h`-style output generation
        oscillator.rs        PLL search and oscillator pragma/init generation
        fuses.rs             Dynamic fuse pragma generation
      parser/
        edc_parser.rs        EDC XML parsing and core device model
        dfp_manager.rs       DFP lookup, extraction, cache roots, overlays
        pack_index.rs        Microchip pack index fetch/cache
        datasheet_fetcher.rs Datasheet resolution and cache/download fallback
        pinout_verifier.rs   LLM-backed datasheet verification
    tests/
      integration.rs
  docs/
  tests/fixtures/
  scripts/release-app.sh
```

## Runtime Flow

### Device load

```text
Frontend `loadDevice()`
  -> invoke("load_device")
  -> dfp_manager::load_device()
     -> find cached device JSON or extracted `.PIC`
     -> optionally fetch/extract matching `.atpack`
     -> parse EDC XML into `DeviceData`
     -> merge `pinouts/*.json` overlays
     -> load `clc_sources/*.json` overrides
  -> frontend renders pin table, package diagram, fuse UI, and CLC UI
```

### Code generation

```text
Frontend collects assignments/settings
  -> invoke("generate_code")
  -> codegen::generate::generate_c_files()
     -> optional oscillator pragmas/init
     -> optional fuse pragmas
     -> optional PPS function
     -> `configure_ports()`
     -> optional op-amp enable function
     -> optional CLC function
     -> `system_init()`
  -> frontend shows/exports the configured `<basename>.c` and `<basename>.h` pair
```

### Verification

```text
Frontend resolves datasheet
  -> invoke("find_datasheet")
  -> local cache / Downloads / Microchip fallback

Frontend sends PDF or cached content
  -> invoke("verify_pinout")
  -> pinout_verifier::verify_pinout()
     -> selects OpenAI or Anthropic from available key
     -> compares parsed pin data against datasheet tables
     -> optionally extracts CLC input-source mappings
```

## Frontend Model

The frontend keeps state in a few long-lived globals spread across ordered browser scripts under `frontend/static/app/`:

- `deviceData`: currently loaded device/package payload from Rust
- `assignments`: pin-position keyed assignment map, including analog-sharing arrays
- `signalNames`: per-pin user aliases used in generated macros
- `generatedFiles`: generated output keyed by filename
- `appSettings`: persisted appearance/startup settings
- `PickleConfig`: unified theme tokens and shell-level UI constants loaded before first paint

The UI is intentionally imperative:

- renderers rebuild DOM sections directly
- undo/redo uses cloned state snapshots
- native menu items are forwarded as Tauri events and handled in the same JS app
- pure state helpers live in `frontend/static/app/model.js` so they can be tested in Node without a DOM
- theme variables and shell copy live in `frontend/static/app/config.js` so CSS and JS do not drift independently

## Backend Modules

| Module | Responsibility |
|---|---|
| `commands.rs` + `commands/*.rs` | Frontend IPC surface, request/response shapes, and command orchestration |
| `settings.rs` | Canonical settings defaults, normalization, and TOML rendering |
| `edc_parser.rs` | Core device structs plus XML parsing and pin resolution |
| `dfp_manager.rs` | Runtime data-root selection, DFP lookup/extraction, overlays, caches |
| `pack_index.rs` | Fetches and caches the Microchip pack catalog |
| `generate.rs` | Builds the generated C source/header pair |
| `oscillator.rs` | PLL search and oscillator-specific generated code |
| `fuses.rs` | Device-driven `#pragma config` generation |
| `pinout_verifier.rs` | Datasheet prompt construction, provider selection, overlay persistence |

## Runtime Directories

pickle reads from multiple roots and writes to the first appropriate mutable root so caches stay colocated with the data the app is already using.

| Directory | Purpose |
|---|---|
| `devices/` | Parsed `DeviceData` JSON cache |
| `dfp_cache/` | Extracted `.atpack` contents and downloaded assets |
| `dfp_cache/datasheets/` | Cached datasheet PDFs/text fallbacks |
| `dfp_cache/verify_cache/` | Cached verification responses |
| `pinouts/` | Manual or verified package overlay JSON |
| `clc_sources/` | Per-device CLC input-source mapping overrides |

## Settings

`settings.toml` lives in the platform app-data directory and currently stores:

- theme mode: `dark`, `light`, or `system`
- startup policy: fixed device or `last-used`
- toolchain policy: fallback compiler plus family-specific overrides for `PIC24` and `dsPIC33`
- last successfully loaded device/package

The backend normalizes casing and whitespace before saving so the file stays stable and diff-friendly.

## Validation

The Rust backend is covered by unit and integration tests in `src-tauri/tests` plus fixture-driven tests under `tests/fixtures/`. A normal repo validation pass is:

```bash
./scripts/validate.sh

## Equivalent manual commands

```bash
cd src-tauri
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
node --check frontend/static/pin_descriptions.js
for file in frontend/static/app/*.js; do node --check "$file"; done
node --test frontend/tests/*.test.js
```
