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
        00-core.js               Core device/editor state, package-label normalization, and device-load orchestration
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
        05-clc-designer.js       CLC editor UI, tab-state gating, register preview, and module orchestration
        05-clc-schematic.js      Deterministic SVG schematic renderer and first-stage router
        05-config-files.js       Config-document lifecycle, dirty state, save/load/rename flows
        05-compile-check.js      Family-aware compiler discovery and compile-check workflows
        06-shell.js              Shell action registry, startup intro, package-action menu, view switching, catalog freshness, and theme/shell chrome
        07-verification-model.js Pure verification progress/package matching helpers
        07-verification-render.js Verification progress/result HTML rendering
        07-verification.js       Datasheet verification IPC flow, overlay application, and verification-panel sync
        08-bootstrap.js          Final startup orchestration, welcome-intro gating, Tauri menu forwarding, and tooltips
      style.css                  Import manifest for the split stylesheet modules
      styles/                    Foundation, component, verification, shell, and responsive CSS modules
      pin_descriptions.js        Pattern-based descriptions and grouping helpers
  src-tauri/
    src/
      main.rs                Tauri bootstrap, plugins, native menu wiring
      commands.rs            Command facade and shared re-exports
      commands/              Concern-focused Tauri command implementations
        types.rs             Shared IPC request/response structs
        support.rs           Small cross-command filesystem/dialog/device helpers
        verification.rs      Verification command facade and re-exports
        verification/
          lookup.rs          Datasheet lookup/download command flow
          run.rs             Pinout and CLC verifier command handlers
          overlay.rs         Overlay mutation and provider-status commands
        verification_support.rs Shared datasheet decode/cache/device-context helpers for verification commands
      part_profile.rs        Shared dsPIC33/PIC24 family and branch trait detection
      settings.rs            TOML-backed appearance/startup settings
      codegen/
        generate.rs          Configurable `mcu_init.c/.h`-style output orchestration
        generate_types.rs    Shared output names plus public generator input structs
        generate_pps.rs      Dedicated `configure_pps()` emission and RP comment labeling
        generate_ports.rs    Dedicated `configure_ports()` / `configure_analog()` emission
        generate_single_file.rs Compile-check/export helper for merged single-file output
        generate_support.rs  Comment alignment, section framing, and text helpers
        generate_clc.rs      CLC data shape, register packing, and emitted CLC function body
        oscillator.rs        Public oscillator facade and family dispatch
        oscillator/
          model.rs          Shared oscillator data model, PLL search, and fuse ownership helpers
          legacy.rs         Legacy dsPIC33CK/PIC24 pragma + PLL SFR generation
          ak.rs             dsPIC33AK runtime clock-generator / PLL1 generation
          tests.rs          Oscillator facade tests
        fuses.rs             Dynamic fuse pragma generation
      parser/
        edc_parser.rs        EDC XML parsing and core device model
        dfp_manager.rs       DFP lookup/extraction plus device/overlay orchestration
        dfp_paths.rs         Shared data-root and cache-path policy
        dfp_datasheet.rs     Datasheet PDF probing, validation, and local cache helpers
        dfp_store.rs         Cached-device JSON, pinout overlays, and CLC source overrides
        pack_index.rs        Microchip pack index fetch/cache
        datasheet_fetcher.rs Datasheet resolution and cache/download fallback
        verifier_cache.rs    Verification-result cache hashing and persistence
        verify_progress.rs   Shared verification progress payloads
        verify_prompt.rs     Verification prompt text, task labels, and cache-scope rules
        verify_pdf.rs        Bookmark scanning, page-range selection, PDF trimming, and PNG rendering
        verify_provider.rs   Provider selection, key lookup, and provider dispatch
        verify_provider_schema.rs Shared structured-output schema for all providers
        verify_provider_anthropic.rs Anthropic upload/request transport and PNG fallback
        verify_provider_openai.rs OpenAI Responses transport, uploads, and PNG fallback
        verify_openai_stream.rs OpenAI streaming response normalization
        verify_compare.rs    Local datasheet-package diffing and branch filtering
        verify_overlay.rs    Overlay persistence, rename/delete, and display-name overrides
        pinout_verifier.rs   Cache-aware verification runner and compare orchestration
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
     -> re-parse stale cached JSON when newer parser features are required
        (for example missing PPS matrices or older dsPIC33AK caches that were
        created before 32-bit SFR inventory support landed, or caches that
        still expose CLC PPS endpoints but report no CLC inventory/module ID)
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
     -> optional oscillator handling
        -> CK-style parts: full pragma/runtime generation
        -> dsPIC33AK: runtime `OSCCFG` / `CLK1CON` / `CLK1DIV` / `PLL1CON` / `PLL1DIV` generation
     -> optional fuse pragmas
     -> optional PPS function
     -> `configure_ports()`
     -> optional op-amp enable function
     -> optional CLC function
        -> CK-style split-register emission
        -> dsPIC33AK: unified 32-bit `CLCxCON` / `CLCxSEL` / `CLCxGLS` emission
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
     -> local PDFs are re-validated against the selected part before reuse so clipped pinout extracts or wrong sibling families do not poison later lookups
     -> once Microchip resolves a specific family datasheet, its cached DS number/title become an additional validation path for later reuse of that family PDF

Frontend sends PDF or cached content
  -> invoke("verify_pinout")
  -> commands/verification.rs
     -> uses commands/verification_support.rs to decode/cache the PDF and build device context
  -> pinout_verifier::verify_pinout()
     -> selects OpenAI or Anthropic from available key
     -> uses verify_prompt.rs to build the provider prompt and stable extraction cache scope
     -> uses verify_provider.rs to select the provider and dispatch to provider-specific transports
     -> uses verify_pdf.rs to find bookmark/text page ranges, trim the PDF, and render PNG fallbacks when needed
     -> uses verify_provider_schema.rs to keep Anthropic/OpenAI on the same structured-output contract
     -> uses verify_provider_anthropic.rs or verify_provider_openai.rs for the actual upload/request flow
     -> reuses cached extraction results by reduced-PDF bytes plus stable extraction scope (pinout pin-count or CLC extraction mode)
     -> normalizes OpenAI streaming output through verify_openai_stream.rs
     -> compares parsed pin data against extracted datasheet tables locally through verify_compare.rs
     -> drops extracted package tables that explicitly target a different device branch than the selected part (for example `MC` tables while verifying an `MPS` device)
     -> drops extracted package tables whose pin count does not match the selected device
     -> optionally extracts CLC input-source mappings
  -> frontend may apply the verified package as an overlay, override the displayed name for any package, and delete overlay-backed packages without mutating built-in EDC package keys
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
- package labels are normalized through the pure frontend model so internal EDC codes and trailing part-family qualifiers can be hidden in the UI, and explicit per-part display-name overrides can rename any package in the UI without changing backend package keys
- the header keeps the current package selector visible even when there is only one visible package, and a shell-owned package-actions menu hangs off that selector for rename/reset/delete actions
- identical datasheet overlay tables are collapsed onto the existing device-pack package key during overlay save/load so shared `VQFN/TQFP` tables do not create redundant duplicate selector entries
- a shared device-profile helper now centralizes family/branch traits such as dsPIC33AK `Fcy = Fosc` behavior instead of scattering raw prefix checks through multiple UI modules
- reservation/conflict policy and pin-presentation rules are split into pure helpers so the renderers and fuse runtime do not each carry their own rule copies
- fuse definitions are normalized and grouped in the pure model before rendering so duplicated AK `ConfigFuseSector` entries do not leak into the UI as repeated raw registers
- config-document state and editor-mutation bookkeeping are centralized so save/dirty/undo redraw behavior is not scattered across every feature module
- CLC mode metadata, saved-state normalization, and register packing live in a pure helper so the designer and schematic renderer do not each own partial copies of the same logic
- the CLC schematic renderer uses deterministic templates plus a constrained router; crossings are acceptable, but different nets must never share the same wire segment
- shell buttons and forwarded native menu items route through a shared shell action registry instead of duplicating dispatch logic in multiple modules
- the first-launch intro is a shell-owned overlay so startup guidance stays in one place instead of being duplicated across every empty right-tab panel

See [CLC](clc.md) for the full CLC subsystem contract, including the shared
frontend/backend data shape, schematic-routing invariants, and generated-code
behavior.

## Backend Modules

| Module | Responsibility |
|---|---|
| `commands.rs` + `commands/*.rs` | Frontend IPC surface, shared IPC types/helpers, command orchestration, and split verification command helpers |
| `settings.rs` | Canonical settings defaults, normalization, and TOML rendering |
| `part_profile.rs` | Shared family, branch, and instruction-cycle traits used by backend codegen, toolchain selection, and datasheet verification |
| `edc_parser.rs` | Core device structs plus XML parsing and pin resolution |
| `dfp_manager.rs` | DFP lookup/extraction, cached-device orchestration, overlays, and compiler-support discovery |
| `dfp_paths.rs` | Shared data-root precedence and cache/overlay path construction |
| `dfp_datasheet.rs` | Datasheet PDF probing, part-family validation, and local datasheet cache helpers |
| `dfp_store.rs` | Cached-device JSON, overlay package merge rules, display-name overrides, and CLC source overrides |
| `pack_index.rs` | Fetches and caches the Microchip pack catalog |
| `generate.rs` | Orchestrates phase-ordered C/header generation and public generator API re-exports |
| `generate_types.rs` | Shared output-name helpers plus public `PinConfig` / `PinAssignment` / output option types |
| `generate_pps.rs` | PPS unlock/lock emission, input/output mapping writes, and RP label comments |
| `generate_ports.rs` | Effective port ownership, ANSEL/TRIS emission, ICSP reservations, and op-amp enables |
| `generate_single_file.rs` | Header-inline merge helpers for compile-check and other single-file consumers |
| `generate_support.rs` | Shared formatting, comment alignment, and fuse-filter helpers for generation phases |
| `generate_clc.rs` | CLC config shape, family-specific register packing, and emitted `configure_clc()` body |
| `oscillator.rs` | Public oscillator facade, family dispatch, and stable re-export surface |
| `oscillator/model.rs` | Shared oscillator model, PLL search, instruction-cycle, and managed-fuse helpers |
| `oscillator/legacy.rs` | Legacy pragma-based oscillator generation for CK-style families |
| `oscillator/ak.rs` | dsPIC33AK runtime clock-generator / PLL1 emission |
| `fuses.rs` | Device-driven `#pragma config` generation |
| `verifier_cache.rs` | Verification-result cache hashing, file layout, disable flag, and JSON persistence keyed by reduced PDF + stable extraction scope |
| `verify_progress.rs` | Shared verification progress/event payloads emitted to the frontend |
| `verify_prompt.rs` | Verification prompt wording, provider/task labels, and stable cache-scope rules |
| `verify_pdf.rs` | Bookmark scanning, text fallback page detection, PDF trimming, and rendered-image fallback preparation |
| `verify_provider.rs` | Provider selection, key lookup, and provider dispatch |
| `verify_provider_schema.rs` | Shared structured-output schema for Anthropic/OpenAI verification calls |
| `verify_provider_anthropic.rs` | Anthropic file upload/request transport plus PNG fallback handling |
| `verify_provider_openai.rs` | OpenAI Responses transport plus PDF/image fallback handling |
| `verify_openai_stream.rs` | OpenAI Responses API streaming assembly and JSON normalization |
| `verify_compare.rs` | Deterministic package-table filtering and local correction generation |
| `verify_overlay.rs` | Overlay JSON persistence, display-name overrides, and overlay package rename/delete helpers |
| `pinout_verifier.rs` | Cache-aware verification runner that coordinates prompt scope, provider transport, and local comparison |

## Runtime Directories

pickle reads from multiple roots and writes to the first appropriate mutable root so caches stay colocated with the data the app is already using.

### App-data root

Typical macOS layout:

```text
~/Library/Application Support/pickle/
  settings.toml
  devices/
    <PART>.json
  pinouts/
    <PART>.json
  clc_sources/
    <PART>.json
  dfp_cache/
    pack_index.json
    Microchip.*.atpack
    edc/
      <PART>.PIC
    datasheets/
      pdf/
        <PART>.pdf
      meta/
        <PART>.json
      text/
        <PART>.md
    verify_cache/
      <hash>.json
```

### Directory reference

| Path | Purpose | Safe to delete? |
|---|---|---|
| `settings.toml` | Theme, startup, onboarding, compiler, codegen, and verification preferences plus last-used device/package | No |
| `devices/` | Parsed `DeviceData` JSON cache after EDC resolution and overlay merge | Yes |
| `pinouts/` | User-managed package overlays, package display-name overrides, and renamed overlay package keys | No |
| `clc_sources/` | User-verified or manual CLC input-source mapping overrides | No |
| `dfp_cache/pack_index.json` | Cached Microchip pack catalog | Yes |
| `dfp_cache/*.atpack` | Downloaded Microchip device family packs | Yes |
| `dfp_cache/edc/` | Extracted `.PIC` files from the cached packs | Yes |
| `dfp_cache/datasheets/pdf/` | Validated datasheet PDF cache keyed by part number | Yes |
| `dfp_cache/datasheets/meta/` | Datasheet resolution metadata: product page URL, document number, revision, sibling-source hint | Yes |
| `dfp_cache/datasheets/text/` | Proxy-extracted datasheet text fallback when PDF download is blocked | Yes |
| `dfp_cache/verify_cache/` | Cached verifier extraction results keyed by PDF hash + provider + stable extraction scope | Yes |

### Cache layout notes

- Current builds write datasheet PDFs only into `dfp_cache/datasheets/pdf/`.
- Older builds may still have flat files at `dfp_cache/datasheets/<PART>.pdf`. Those are legacy cache artifacts; once the same part exists under `datasheets/pdf/`, the flat file is redundant.
- `devices/`, `dfp_cache/edc/`, and `dfp_cache/datasheets/` are performance caches. Deleting them forces reparse/redownload but does not destroy user work.
- `pinouts/` and `clc_sources/` are the only runtime directories that should be treated as user-authored project data.

## Settings

`settings.toml` lives in the platform app-data directory and currently stores:

- theme mode: `dark`, `light`, or `system`
- startup policy: fixed device or `last-used`
- onboarding state: whether the first-launch intro has been dismissed
- toolchain policy: fallback compiler plus family-specific overrides for `PIC24` and `dsPIC33`
- last successfully loaded device/package

The backend normalizes casing and whitespace before saving so the file stays stable and diff-friendly.

Browser-engine storage under `~/Library/WebKit/<bundle-id>/` may still exist for
WebKit internals such as search history and media-key salts, but app-owned
state should live in `settings.toml`, the keychain, or the documented runtime
directories under the app-data root.

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
