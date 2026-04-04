# pickle — Anthropic Code Guidelines

## Project Overview

Native desktop pin multiplexing configurator for Microchip dsPIC33 and PIC24 devices, built with Rust/Tauri. Parses Device Family Pack (DFP) `.atpack` files, presents an interactive pin assignment UI, and generates MISRA C:2012 compliant initialization code. This is the Rust port of the Python/FastAPI `config-pic` project.

## Architecture

```
pickle/
  src-tauri/
    src/
      main.rs            # Tauri entry point
      commands.rs        # Shared command types/helpers plus module exports
      commands/          # Concern-focused #[tauri::command] implementations
      lib.rs             # Library root, module declarations
      parser/
        mod.rs
        edc_parser.rs    # Parses .PIC (XML) files into DeviceData
        dfp_manager.rs   # Finds/extracts .atpack files, caches devices
        pack_index.rs    # Microchip pack index fetch/cache
        pinout_verifier.rs # LLM-based pinout verification + CLC source extraction
        datasheet_fetcher.rs # Microchip datasheet PDF resolution and caching
      codegen/
        mod.rs
        generate.rs      # PPS, TRIS, ANSEL, op-amp, CLC C code generation
        oscillator.rs    # PLL calculation and oscillator pragma generation
        fuses.rs         # Fuse pragma generation
    Cargo.toml
    tauri.conf.json
    build.rs
  frontend/              # Served by Tauri webview
    index.html
    static/
      app/
        config.js        # Unified frontend theme tokens and shell-level UI constants
        model.js         # Pure frontend state/normalization helpers
        00-*.js          # Ordered browser scripts for state, views, workflows, bootstrap
      style.css              # CSS import manifest
      styles/                # Split stylesheet modules loaded by style.css
      pin_descriptions.js
  tests/
    fixtures/            # Test device JSON files
  pinouts/               # Pinout overlay JSON files (LLM-verified or manual)
  clc_sources/           # CLC input source mapping overrides (per-device)
```

## Key Data Flow

1. User enters part number -> `invoke('load_device', ...)` -> `dfp_manager::load_device()` -> `edc_parser::parse_edc_file()`
   - Pinout overlays from `pinouts/*.json` are merged at load time
   - CLC input source mapping is loaded from `clc_sources/` or extracted by LLM during verification
2. Frontend renders package diagram + pin table from resolved pin data
3. User assigns peripherals -> frontend collects assignments
4. User configures CLC modules via the CLC designer tab -> selects logic mode, input sources, gate connections
5. "Generate" -> `invoke('generate_code', ...)` -> `codegen::generate::generate_c_files()` -> C source returned

## Development

```bash
cd src-tauri && cargo test          # Run unit tests
cargo tauri dev                     # Start app in dev mode
cargo tauri build                   # Production build
```

## Conventions

- **Rust**: Edition 2021, all structs derive `Debug, Clone, Serialize, Deserialize`. Use `thiserror` or `String` for errors.
- **Frontend**: Vanilla JS, no build step, no framework. All state in global variables. Uses `invoke()` instead of `fetch()`.
- **Frontend config**: `frontend/static/app/config.js` is the source of truth for theme tokens, typography, shell copy, and UI timings. Do not add new hard-coded UI constants in random JS/CSS files when they belong there.
- **CSS**: `style.css` is the stable entrypoint and should only coordinate the split `frontend/static/styles/*.css` files. Those modules should consume config-driven CSS variables instead of re-defining theme palettes inline. Peripheral colors still flow through tokens like UART=`--uart`, SPI=`--spi`, I2C=`--i2c`, PWM=`--pwm`.
- **Code generation output**: MISRA C:2012 compliant C99. All register values use `U` suffix. Comments explain every write.
- **No auto-commit**: Never commit or push without explicit user permission.

## Commenting Strategy

- **Current state**: top-level documentation is now expected in every non-trivial source file, but comment quality should still be reviewed whenever logic becomes denser or more cross-coupled.
- **Comment the why, not the obvious**: do not add noise like "increment counter" or "set variable". Add comments when the intent, invariant, workaround, or hardware/domain rule would not be obvious from the code alone.
- **Start each non-trivial source file with a short file/module header**:
  - Rust: use `//!` module docs
  - JS: use a top-of-file block comment
  - The header should say the file's responsibility, the key data it owns, and the main boundaries/dependencies
- **Future standard**:
  - treat file headers plus intent-level comments for risky blocks as mandatory, not optional
  - when splitting code into new files, add the header in the same patch
  - when removing or changing behavior, update nearby comments immediately so stale comments do not accumulate
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
  - `frontend/static/app/05-config-files.js`: restore/load flows and config persistence boundaries
  - `frontend/static/app/05-clc-designer.js`: dense CLC state/rendering logic and register preview coupling
  - `frontend/static/app/05-compile-check.js`: toolchain detection, compiler UX, backend/UI contract
  - `frontend/static/app/06-shell.js`: shell event wiring, catalog freshness state, theme/app-shell coupling
  - `frontend/static/app/07-verification.js`: datasheet verification rendering, overlay application, verifier/UI contract
  - `frontend/static/app/08-bootstrap.js`: startup/menu/tooltip orchestration boundaries

## Important Patterns

- `DeviceData` (in `edc_parser.rs`) is the central data model — pads, pinouts, PPS mappings, port registers, ANSEL bits.
- `DeviceData::resolve_pins(package)` returns the pin list for a specific package variant.
- ICSP pins (MCLR, PGC1, PGD1) are detected by regex and excluded from ANSEL/TRIS code generation — only a reservation comment is emitted.
- Pinout overlays in `pinouts/*.json` add alternate package variants not present in the EDC file.
- Tauri IPC: frontend uses `invoke('command_name', {args})`, backend uses `#[tauri::command]`.
- All HTTP calls happen in Rust (reqwest), not from the frontend.
- CLC input source mapping (`clc_input_sources`) is device-specific and loaded from `clc_sources/*.json`. When not available locally, the LLM verification flow extracts it from the datasheet and saves it for future use.
- LLM verification supports dual providers: Anthropic (Anthropic) and OpenAI. The `api_key_status` command reports which providers are configured, and the frontend lets the user choose.

## Microchip dsPIC33 Domain Knowledge

- PPS (Peripheral Pin Select): remappable I/O via RPINR/RPOR registers, requires RPCON unlock/lock.
- FICD.ICS selects the active ICSP debug pair (PGC1/PGD1 is factory default).
- Part number format: `DSPIC33CK64MP102T-E/M6VAO` = base + T(tape/reel) + temp grade + /package + VAO(automotive).
- Config fuses: `#pragma config` for FICD, FWDT, FOSCSEL, FOSC, FBORPOR.
- CLC (Configurable Logic Cell): up to 4 modules, each with 4 data select inputs, 4 gates, and a configurable logic function. Configured via CLCnCON, CLCnSEL, and CLCnGLS registers.

## What NOT to Do

- Don't add npm/webpack/bundler tooling — the frontend is intentionally dependency-free.
- Don't mock the EDC parser in integration tests — use real DeviceData fixtures.
- Don't generate ANSEL/TRIS writes for ICSP debug pins.
- Don't auto-format generated C code — the output formatting is intentional.
- C code output from Rust must be byte-identical to the Python version.
