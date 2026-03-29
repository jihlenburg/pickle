# Architecture

## Project Structure

```
pickle/
  scripts/
    release-app.sh           Builds `pickle.app` and stages it into `bin/`
  src-tauri/
    src/
      main.rs                 Tauri entry point
      lib.rs                  Library root
      commands.rs             Tauri IPC command handlers (14 commands)
      parser/
        edc_parser.rs         Parses .PIC (EDC XML) files into DeviceData
        dfp_manager.rs        Finds/extracts .atpack files, JSON caching, overlay loading
        pack_index.rs         Microchip pack index fetch, parse, and cache
        pinout_verifier.rs    Anthropic API pinout cross-check against datasheet PDFs
      codegen/
        generate.rs           PPS, TRIS, ANSEL, op-amp C code generation
        oscillator.rs         PLL divider calculation and oscillator pragma generation
        fuses.rs              Configuration fuse pragma generation
    Cargo.toml
    tauri.conf.json
    icons/
  frontend/                   Served by Tauri webview (no build step)
    index.html
    static/
      app.js                  All UI logic, Tauri invoke() IPC calls, native dialog flows
      style.css               Dark/light theme, peripheral color coding
      pin_descriptions.js     Human-readable peripheral descriptions
  tests/
    fixtures/                 Test device JSON files
```

## Data Flow

```
User enters part number
  -> invoke('load_device')
  -> dfp_manager::load_device()
     -> Checks JSON cache in devices/
     -> Falls back to .atpack extraction via edc_parser::parse_edc_file()
     -> Auto-downloads pack from Microchip index if not found locally
     -> Applies pinout overlays from pinouts/*.json
  -> Frontend renders package diagram + pin table

User assigns peripherals via dropdowns
  -> Frontend tracks assignments in memory

User clicks "Generate C Code"
  -> invoke('generate_code')
  -> codegen::generate::generate_c_files()
     -> PPS unlock/lock register writes
     -> TRIS / LAT / ANSEL / ODC port configuration
     -> ICSP pins (MCLR, PGCn, PGDn) excluded — reservation comment only
     -> oscillator.rs / fuses.rs append #pragma config sections
  -> Returns { "pin_config.c": "...", "pin_config.h": "..." }
```

## Key Data Model

| Struct | File | Purpose |
|---|---|---|
| `DeviceData` | `edc_parser.rs` | Central model: pads, pinouts, PPS mappings, port registers |
| `Pad` | `edc_parser.rs` | Physical pin: functions, RP number, port/bit, analog channels |
| `Pinout` | `edc_parser.rs` | Package variant: pin count, position-to-pad mapping |
| `ResolvedPin` | `edc_parser.rs` | Fully resolved pin for a specific package |
| `PinConfig` | `generate.rs` | User's complete pin configuration for code generation |
| `PinAssignment` | `generate.rs` | Single peripheral-to-pin assignment |
| `OscConfig` | `oscillator.rs` | Oscillator settings: source, target frequency, crystal |
| `PLLResult` | `oscillator.rs` | Computed PLL dividers and resulting frequencies |
| `FuseConfig` | `fuses.rs` | Configuration fuse selections (ICSP, WDT, BOR) |
| `PackIndex` | `pack_index.rs` | Cached Microchip pack index with device lookup |
| `VerifyResult` | `pinout_verifier.rs` | Pinout verification result from Anthropic API |

## Frontend

The UI is vanilla HTML/CSS/JS with no build step and no framework. All state lives in global variables (`deviceData`, `assignments`, `signalNames`, `generatedFiles`). The frontend communicates with the Rust backend exclusively through Tauri's `invoke()` IPC — there are no REST endpoints or HTTP calls from the browser. File open/save/export flows also go through Rust commands backed by `tauri-plugin-dialog`, so config loading, pin-list export, generated-code export, and datasheet PDF selection use native desktop pickers instead of browser `Blob` or hidden file-input logic.

Undo/redo is supported via `undoStack` / `redoStack` arrays.

### Theming

CSS custom properties in `:root` support dark (default) and light themes. Peripheral types are color-coded:

| Peripheral | CSS variable |
|---|---|
| UART | `--uart` |
| SPI | `--spi` |
| I2C | `--i2c` |
| PWM | `--pwm` |
| ADC | `--adc` |
| Timer | `--timer` |

## Runtime Directories

pickle can read runtime data from multiple roots:

- the current repo root
- a sibling `../config-pic` checkout when present
- the Tauri app-data directory as fallback

For mutable data (`devices/`, `dfp_cache/`, `pinouts/`), writes go to the first existing matching root so the desktop app can share caches and overlays with the working `config-pic` app in this workspace.

These directories are used at runtime and excluded from version control:

| Directory | Purpose |
|---|---|
| `dfp_cache/` | Extracted `.atpack` contents (EDC XML files) |
| `devices/` | Parsed device data cached as JSON |
| `pinouts/` | Pinout overlay files (corrections from verification or manual edits) |

## Release Staging

Use `./scripts/release-app.sh` to build the macOS `.app` bundle and copy the latest `pickle.app` into `./bin/`. The script intentionally runs `cargo tauri build --bundles app` so it skips the known-bad DMG bundling path on macOS 26 while still staging a current app bundle for local use.

## Dependencies

| Crate | Purpose |
|---|---|
| `tauri` 2 | Desktop app framework, IPC |
| `serde` / `serde_json` | Serialization for all structs and IPC |
| `roxmltree` | XML parsing for EDC `.PIC` files |
| `zip` | `.atpack` (ZIP) extraction |
| `reqwest` (blocking) | HTTP for pack index and pack downloads |
| `regex` | Part number matching, pad name parsing |
| `chrono` | Cache age calculation |
| `base64` | PDF encoding for Anthropic API |
| `dirs` | Home directory / cache path detection |
| `dotenvy` | `.env` file for API keys |
| `lopdf` | PDF page extraction |
| `tempfile` | Temp directory for compile checks |

## Project History

pickle is a Rust/Tauri port of [config-pic](https://github.com/jihlenburg/config-pic), originally written in Python with FastAPI. The port replaces the web server with native IPC, `fetch()` with `invoke()`, and delivers the app as a standalone desktop binary.

## Test Coverage

26 unit tests across all code generation and parser modules:

| Module | Tests | Coverage |
|---|---|---|
| `edc_parser` | 6 | Parse int, port info, RP number, canonical name, JSON round-trip, resolve pins |
| `oscillator` | 12 | PLL targets (100/140/200 MHz), crystal inputs, VCO/FPFD constraints, divider ranges, unreachable frequency, pragma generation |
| `fuses` | 4 | Default/custom fuses, section completeness, field coverage |
| `generate` | 4 | Multi-file output, ICSP exclusion, call order, PPS lock/unlock |
