# pickle — Logbook

## 2026-03-29

### Project Initialization
- Created Rust/Tauri project structure as port of Python config-pic
- Scaffolded all module files matching Python structure
- Copied frontend assets (HTML, CSS, JS) from config-pic
- Created AGENTS.md, TODO.md, LOGBOOK.md
- Set up Cargo.toml with all dependencies (tauri, serde, roxmltree, zip, reqwest, regex, etc.)

### Data Model (Phase 2)
- Ported all data structures from `edc_parser.py` to Rust structs with serde derives
- Implemented `DeviceData::from_json()`/`to_json()` with JSON round-trip support
- Handled Python's int-keyed dicts (Pinout.pins) via custom serde for `HashMap<u32, String>`

### Code Generation (Phase 3)
- Ported `codegen/fuses.rs` — fuse pragma generation for FICD, FWDT, FBORPOR
- Ported `codegen/oscillator.rs` — PLL calculator with brute-force divider search
- Ported `codegen/generate.rs` — full C code generation with comment alignment, ICSP handling, multi-file output
- Ported all unit tests from Python test suite

### EDC Parser (Phase 4)
- Ported `parser/edc_parser.rs` — XML parsing with roxmltree, namespace handling
- Pad canonical naming, RP number extraction, port info extraction

### DFP Manager + Pack Index (Phase 5)
- Ported `parser/dfp_manager.rs` — .atpack search/extract, JSON caching, overlay loading
- Ported `parser/pack_index.rs` — Microchip index fetch/parse/cache

### Pinout Verifier (Phase 7)
- Ported `parser/pinout_verifier.rs` — Anthropic API integration, PDF page extraction

### Tauri Commands + Frontend (Phase 6)
- Implemented all `#[tauri::command]` handlers in `commands.rs`
- Updated `app.js` to use `invoke()` instead of `fetch()`

### Compilation + Bug Fixes
- Installed Rust toolchain (1.94.1)
- Fixed: removed dead `lazy_static!` macro in `dfp_manager.rs`
- Fixed: moved `use base64::Engine` import before usage in `commands.rs`
- Fixed: added missing `tempfile` crate to `Cargo.toml`
- Fixed: added `json` feature to `reqwest` dependency
- Fixed: added type annotations for `Value` closures in `pinout_verifier.rs`
- Fixed: removed unused `has_ns` function from `edc_parser.rs`
- Fixed: removed unused `generate_c_code` import from `commands.rs`
- Fixed: `test_unreachable_frequency` — PLL can actually reach 1 GHz with VCO max 1.6 GHz
- Generated placeholder app icons
- **Clean build: 0 warnings, 26/26 tests passing**

### Menu Bar
- Added native menu bar: File (Open/Save/Export/Quit), Edit (Undo/Redo/Cut/Copy/Paste), View (Generate/Copy Code), Help (About)
- Menu events emitted to frontend via `app.emit("menu-action", id)`, listened by `window.__TAURI__.event.listen()`
- Keyboard shortcuts: Cmd+O, Cmd+S, Cmd+E, Cmd+G, Cmd+Z, Cmd+Shift+Z, Cmd+Shift+C

### Integration Tests
- Added 7 end-to-end tests in `tests/integration.rs` using the DSPIC33CK64MP102 fixture:
  - Device fixture loading and validation
  - Pin resolution for default and all packages
  - UART PPS code generation with unlock/lock verification
  - Oscillator + fuse pragma generation
  - JSON round-trip data preservation
  - Signal name macro generation
- Fixed: PPS direction values use `"in"`/`"out"` (not `"input"`/`"output"`)
- **All 33 tests passing (26 unit + 7 integration)**

### Production Build
- Installed `tauri-cli` v2.10.1
- `cargo tauri build --bundles app` produces 16 MB `pickle.app` bundle
- DMG bundling fails on macOS 26 (Tahoe) due to `create-dmg` script argument issue — `.app` bundle works fine
- App icon: pickle-shaped DIP IC chip (SVG -> PNG -> .icns/.ico)

### Data Root Parity Fixes
- Fixed Rust data-root resolution so pickle reads from the repo root, sibling `../config-pic`, and app-data fallback instead of app-data only
- Fixed write-path selection so overlays and caches are written back into the first existing matching data root, preserving parity with the working `config-pic` checkout
- Aligned `.env` lookup for `ANTHROPIC_API_KEY` with the shared workspace roots
- Added startup logging for read roots, cache directories, and pinout directory selection

### Native File Dialogs + UI Polish
- Added native Tauri dialog commands for opening text files, opening binary files, saving text files, and exporting generated files to a chosen folder
- Replaced browser-style file input, `Blob`, and object URL flows in the frontend with native desktop open/save/export paths
- Updated the device catalog badge to show freshness, staleness, and cache age
- Polished the desktop UI shell: sticky header, clearer panel hierarchy, improved status badges, refined action buttons, and responsive layout cleanup

### Release Staging
- Added `scripts/release-app.sh` to build `pickle.app` and copy the latest bundle into `./bin/pickle.app`
- Script uses `cargo tauri build --bundles app` to avoid the known DMG bundling hang while still staging a fresh local app bundle
- Verified `bin/pickle.app` was refreshed from the latest release bundle
