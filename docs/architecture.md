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
        config.js                Unified theme tokens, typography, and shell-level UI constants
        model.js                 Pure frontend state and normalization helpers
        00-core.js               Core device/editor state and device-load orchestration
        01-reservation-policy.js Pure reservation/conflict policy helpers
        01-reservations.js       Fuse-coupled reservation runtime and conflict highlighting
        02-view-model.js         Shared pin-presentation helpers for left-panel renderers
        02-peripheral-view.js    Peripheral-centric renderer built on the shared view model
        03-pin-view.js           Pin table + package renderer built on the shared view model
        04-editor-state.js       Shared mutation bookkeeping for undo/redraw/dirty flows
        04-interactions.js       Interactive assignment handlers
        05-codegen.js            Code generation, code tabs, clipboard copy, plain-text export
        05-device-config.js      Oscillator/fuse UI state and hardware-reservation coupling
        05-clc-model.js          Pure CLC state defaults, normalization, and register packing
        05-clc-designer.js       CLC editor UI, register preview, and module orchestration
        05-clc-schematic.js      Deterministic SVG schematic renderer and first-stage router
        05-config-files.js       Config-document lifecycle, dirty state, save/load/rename flows
        05-compile-check.js      Family-aware compiler discovery and compile-check workflows
        06-shell.js              Shell action registry, view switching, catalog freshness, theme/shell chrome
        07-verification.js       Datasheet verification flow and overlay application
        08-bootstrap.js          Final startup orchestration, Tauri menu forwarding, and tooltips
      style.css                  Import manifest for the split stylesheet modules
      styles/                    Foundation, component, verification, shell, and responsive CSS modules
      pin_descriptions.js        Pattern-based descriptions and grouping helpers
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

### Config documents

```text
Frontend edit mutates device state
  -> 05-config-files.js recomputes serialized JSON payload
  -> configDocument dirty state updates title/header affordances

User triggers Save
  -> existing path: invoke("write_text_file_path")
  -> first save / Save As / Rename: invoke("save_text_file_dialog")
  -> Rename also invokes("delete_file_path") for the superseded path when possible
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
- `configDocument`: current config-file path plus last-saved serialized contents

The UI is intentionally imperative:

- renderers rebuild DOM sections directly
- undo/redo uses cloned state snapshots
- native menu items are forwarded as Tauri events and handled in the same JS app
- pure state helpers live in `frontend/static/app/model.js` so they can be tested in Node without a DOM
- theme variables and shell copy live in `frontend/static/app/config.js` so CSS and JS do not drift independently
- reservation/conflict policy and pin-presentation rules are split into pure helpers so the renderers and fuse runtime do not each carry their own rule copies
- config-document state and editor-mutation bookkeeping are centralized so save/dirty/undo redraw behavior is not scattered across every feature module
- CLC mode metadata, saved-state normalization, and register packing live in a pure helper so the designer and schematic renderer do not each own partial copies of the same logic
- the CLC schematic renderer uses deterministic templates plus a constrained router; crossings are acceptable, but different nets must never share the same wire segment
- shell buttons and forwarded native menu items route through a shared shell action registry instead of duplicating dispatch logic in multiple modules

See [CLC](clc.md) for the full CLC subsystem contract, including the shared
frontend/backend data shape, schematic-routing invariants, and generated-code
behavior.

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
```

## Equivalent manual commands

```bash
cd src-tauri
cargo test
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cd ..
node --check frontend/static/pin_descriptions.js
for file in frontend/static/app/*.js; do node --check "$file"; done
node --test frontend/tests/*.test.js
```
