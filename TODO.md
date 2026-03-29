# pickle — TODO (Rust/Tauri Port)

## Phase 1 — Scaffold — Done
- [x] Initialize Tauri project structure
- [x] Copy frontend files (HTML, CSS, JS)
- [x] Create Rust module stubs
- [x] Add crate dependencies to Cargo.toml
- [x] Create AGENTS.md, TODO.md, LOGBOOK.md

## Phase 2 — Data Model + Serialization — Done
- [x] Define all structs in `edc_parser.rs` with serde derives
- [x] Implement `DeviceData::from_json()` / `to_json()`
- [x] Test: load `DSPIC33CK64MP102.json`, verify round-trip

## Phase 3 — Code Generation — Done
- [x] Port `codegen/fuses.rs`
- [x] Port `codegen/oscillator.rs` (PLL algorithm + pragma generation)
- [x] Port `codegen/generate.rs` (PPS, TRIS, ANSEL, multi-file output)
- [x] Port all tests from Python (test_codegen, test_fuses, test_pll)

## Phase 4 — EDC Parser — Done
- [x] Port `parser/edc_parser.rs` (XML namespace handling with roxmltree)
- [x] Test: parse `.PIC` file, compare against cached JSON

## Phase 5 — DFP Manager + Pack Index — Done
- [x] Port `parser/pack_index.rs` (HTTP fetch, XML parse, JSON cache)
- [x] Port `parser/dfp_manager.rs` (ZIP extraction, overlay loading, cache)

## Phase 6 — Tauri Commands + Frontend Wiring — Done
- [x] Implement all `#[tauri::command]` handlers in `commands.rs`
- [x] Update `app.js`: replace all `fetch()` with `invoke()`
- [x] Integration tests: load fixture, assign pins, generate code, verify output

## Phase 7 — Pinout Verifier — Done
- [x] Port `parser/pinout_verifier.rs` (Anthropic API, PDF handling)

## Phase 8 — Polish + Distribution — Done
- [x] App icon (pickle-shaped DIP IC chip)
- [x] Menu bar (File: Open/Save/Export/Quit, Edit: Undo/Redo/Cut/Copy/Paste, View: Generate/Copy Code, Help: About)
- [x] `cargo tauri build` — `.app` bundle builds (16 MB release binary)
- [x] `./scripts/release-app.sh` stages latest `pickle.app` into `./bin`
- [ ] DMG bundling (fails on macOS 26 due to `create-dmg` script issue — `.app` works fine)

## Testing — Done
- [x] 26 unit tests passing (parser, codegen, oscillator, fuses)
- [x] 7 integration tests passing (fixture load, pin resolution, code generation, signal macros, oscillator+fuses)
- [x] Clean build: 0 warnings

## Backlog (inherited from config-pic)
- [ ] Peripheral-centric view
- [ ] Interrupt vector stub generation
