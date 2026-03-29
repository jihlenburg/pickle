# pickle — Anthropic Code Guidelines

## Project Overview

Native desktop pin multiplexing configurator for Microchip dsPIC33 microcontrollers, built with Rust/Tauri. Parses Device Family Pack (DFP) `.atpack` files, presents an interactive pin assignment UI, and generates MISRA C:2012 compliant initialization code. This is the Rust port of the Python/FastAPI `config-pic` project.

## Architecture

```
pickle/
  src-tauri/
    src/
      main.rs            # Tauri entry point
      commands.rs        # All #[tauri::command] handlers
      lib.rs             # Library root, module declarations
      parser/
        mod.rs
        edc_parser.rs    # Parses .PIC (XML) files into DeviceData
        dfp_manager.rs   # Finds/extracts .atpack files, caches devices
        pack_index.rs    # Microchip pack index fetch/cache
        pinout_verifier.rs # Anthropic API pinout verification
      codegen/
        mod.rs
        generate.rs      # PPS, TRIS, ANSEL, op-amp C code generation
        oscillator.rs    # PLL calculation and oscillator pragma generation
        fuses.rs         # Fuse pragma generation
    Cargo.toml
    tauri.conf.json
    build.rs
  frontend/              # Served by Tauri webview
    index.html
    static/
      app.js
      style.css
      pin_descriptions.js
  tests/
    fixtures/            # Test device JSON files
```

## Key Data Flow

1. User enters part number -> `invoke('load_device', ...)` -> `dfp_manager::load_device()` -> `edc_parser::parse_edc_file()`
2. Frontend renders package diagram + pin table from resolved pin data
3. User assigns peripherals -> frontend collects assignments
4. "Generate" -> `invoke('generate_code', ...)` -> `codegen::generate::generate_c_files()` -> C source returned

## Development

```bash
cd src-tauri && cargo test          # Run unit tests
cargo tauri dev                     # Start app in dev mode
cargo tauri build                   # Production build
```

## Conventions

- **Rust**: Edition 2021, all structs derive `Debug, Clone, Serialize, Deserialize`. Use `thiserror` or `String` for errors.
- **Frontend**: Vanilla JS, no build step, no framework. All state in global variables. Uses `invoke()` instead of `fetch()`.
- **CSS**: CSS custom properties in `:root`, supports dark/light/system themes. Peripheral colors: UART=`--uart`, SPI=`--spi`, I2C=`--i2c`, PWM=`--pwm`.
- **Code generation output**: MISRA C:2012 compliant C99. All register values use `U` suffix. Comments explain every write.
- **No auto-commit**: Never commit or push without explicit user permission.

## Commenting Strategy

- **Current state**: top-level documentation is decent in `frontend/static/app.js`, `src-tauri/src/commands.rs`, and the parser modules, but comment quality is not yet uniform across the codebase. The main gap is in complex logic where the code says *what* it does but not always *why* it does it.
- **Comment the why, not the obvious**: do not add noise like "increment counter" or "set variable". Add comments when the intent, invariant, workaround, or hardware/domain rule would not be obvious from the code alone.
- **Start each non-trivial source file with a short file/module header**:
  - Rust: use `//!` module docs
  - JS: use a top-of-file block comment
  - The header should say the file's responsibility, the key data it owns, and the main boundaries/dependencies
- **Document public shapes and quirks**:
  - Use `///` on Rust structs/functions when the semantics are non-obvious
  - In JS, use short JSDoc on state objects and major functions
  - Always document serialization quirks, naming mismatches, and data-shape conversions
- **Add comments before complex blocks, not inside every line**:
  - good targets: parser passes, regex-heavy classification, data-root selection, pack-index caching, codegen ordering, overlay merge rules, and UI state restoration
  - prefer a 1-3 line preamble explaining the block's purpose and constraints
- **Call out invariants explicitly**:
  - examples: why ICSP pins are excluded from generated writes, why writes go to the first existing data root, why overlay pin names may differ from base pad names, why menu events are re-emitted into the frontend
- **Use comments to explain cross-file coupling**:
  - if backend field naming must match frontend expectations, or if a command exists only to support a specific UI flow, leave a short note near that boundary
- **Keep comments close to risk**:
  - put comments next to edge cases, fallback behavior, hardware assumptions, and compatibility hacks
  - do not centralize all explanations in one giant file comment if the risk lives in a specific function
- **Prefer tests for behavioral truth, comments for intent**:
  - if something is subtle and must not regress, add or update a test
  - comments should explain the reasoning; tests should prove the behavior
- **When editing existing code**:
  - preserve useful comments
  - remove stale comments if behavior changes
  - if a change adds non-obvious logic, add the comment in the same patch
- **High-value places to improve over time**:
  - `src-tauri/src/parser/dfp_manager.rs`: root selection, cache lookup order, overlay/load precedence
  - `src-tauri/src/parser/edc_parser.rs`: parsing passes, PPS extraction rules, canonical pad fallback
  - `src-tauri/src/codegen/generate.rs`: generation phases, ordering constraints, ICSP exclusions, analog/digital decisions
  - `frontend/static/app.js`: restore/load flows, catalog freshness state, verification/apply-overlay path

## Important Patterns

- `DeviceData` (in `edc_parser.rs`) is the central data model — pads, pinouts, PPS mappings, port registers, ANSEL bits.
- `DeviceData::resolve_pins(package)` returns the pin list for a specific package variant.
- ICSP pins (MCLR, PGC1, PGD1) are detected by regex and excluded from ANSEL/TRIS code generation — only a reservation comment is emitted.
- Pinout overlays in `pinouts/*.json` add alternate package variants not present in the EDC file.
- Tauri IPC: frontend uses `invoke('command_name', {args})`, backend uses `#[tauri::command]`.
- All HTTP calls happen in Rust (reqwest), not from the frontend.

## Microchip dsPIC33 Domain Knowledge

- PPS (Peripheral Pin Select): remappable I/O via RPINR/RPOR registers, requires RPCON unlock/lock.
- FICD.ICS selects the active ICSP debug pair (PGC1/PGD1 is factory default).
- Part number format: `DSPIC33CK64MP102T-E/M6VAO` = base + T(tape/reel) + temp grade + /package + VAO(automotive).
- Config fuses: `#pragma config` for FICD, FWDT, FOSCSEL, FOSC, FBORPOR.

## What NOT to Do

- Don't add npm/webpack/bundler tooling — the frontend is intentionally dependency-free.
- Don't mock the EDC parser in integration tests — use real DeviceData fixtures.
- Don't generate ANSEL/TRIS writes for ICSP debug pins.
- Don't auto-format generated C code — the output formatting is intentional.
- C code output from Rust must be byte-identical to the Python version.
