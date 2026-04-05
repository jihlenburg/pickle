# pickle

<p align="center">
  <img src="src-tauri/icons/pickle-icon-master.png" width="128" height="128" alt="pickle logo — a kawaii pickle with SO-8 metallic pins">
</p>

**pickle** is a native desktop pin configurator for Microchip dsPIC33 and PIC24 devices. It parses Microchip Device Family Pack (`.atpack`) data, renders package-aware pin assignment views, and generates compiler-friendly initialization code for PPS, port direction, oscillator, fuse, and CLC setup.

The name is a pun on **PIC** — Microchip's microcontroller line.

<!-- Keep this disclaimer block intact; tests/readme.test.js validates it. -->
<!-- mandatory-readme-legal-start -->
## Legal Disclaimer

pickle is an independent project and has no affiliation with, endorsement from, sponsorship from, or approval by Microchip Technology Inc.

All Microchip intellectual property referenced by this project, including Microchip, dsPIC33, PIC24, and related product names, trademarks, and brand names, belongs to Microchip Technology Inc.
<!-- mandatory-readme-legal-end -->

## What It Covers

- Interactive pin-table and peripheral-centric assignment views
- Package diagrams for DIP, SSOP, QFN, and QFP-style layouts
- PPS generation with explicit unlock/lock handling
- Port-mode generation for `ANSELx` and `TRISx`
- Optional oscillator pragma/init generation and dynamic fuse pragmas
- CLC designer with schematic/register preview and generated `CLCn*` writes
- Native open/save/export dialogs via Tauri
- Optional family-aware compile checks (`xc16-gcc` for PIC24, `xc-dsc-gcc` for dsPIC33) using installed or cached device packs, plus LLM-assisted datasheet verification

## Config Files

The app treats saved pin configurations as normal documents:

- `Save` writes back to the current path without reopening a dialog
- first save falls back to the native save dialog
- `Save As...` and `Rename...` are available from the split save menu and the native File menu
- the header shows the current config name plus an unsaved-changes indicator
- `Ctrl/Cmd+S` follows the same direct-save behavior as the toolbar/menu action

## Repo Layout

- `frontend/`: static HTML/CSS/JS loaded directly by the Tauri webview
- `frontend/static/app/`: ordered browser modules for state, pure policy/view/model helpers, document lifecycle, codegen/CLC workflows, shell actions, and verification
- `src-tauri/`: Rust backend, parser, code generator, settings, and Tauri shell
- `docs/`: architecture notes, command contracts, code-generation behavior, and domain notes
- `tests/fixtures/`: fixture device JSON used by integration tests

## Frontend Config

Frontend polish settings live in [`frontend/static/app/config.js`](frontend/static/app/config.js).

- Theme palettes and CSS tokens are defined there and applied before first paint.
- UI constants such as theme-cycle labels, badge copy, and interaction timings are defined there.
- Future frontend polish work should update `config.js` first instead of adding new hard-coded values in JS or CSS.

## Quick Start

Prerequisites:

- Rust via `rustup`
- Tauri CLI via `cargo install tauri-cli`
- Optional: Microchip compilers for compile checks (`xc16-gcc` for PIC24, `xc-dsc-gcc` for dsPIC33)
- Optional: `OPENAI_API_KEY` or `ANTHROPIC_API_KEY` in the environment or repo-root `.env` for pinout verification

```bash
git clone https://github.com/jihlenburg/pickle.git
cd pickle

./scripts/validate.sh
cargo tauri dev
cargo tauri build
./scripts/release-app.sh
```

## Validation

```bash
./scripts/validate.sh
```

If you want the individual checks instead of the wrapper script:

```bash
cd src-tauri
cargo fmt --all -- --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cd ..

node --check frontend/static/pin_descriptions.js
for file in frontend/static/app/*.js; do node --check "$file"; done
node --test frontend/tests/*.test.js
```

## Settings And Runtime Data

pickle stores behavior settings in `settings.toml` under the platform app data directory:

- macOS: `~/Library/Application Support/pickle/settings.toml`
- Linux: `~/.local/share/pickle/settings.toml`
- Windows: `%APPDATA%\\pickle\\settings.toml`

The backend also manages mutable runtime caches and overlays in the first matching data root it can write to, including:

- `devices/`
- `dfp_cache/`
- `pinouts/`
- `clc_sources/`

`settings.toml` also persists compiler preferences under `[toolchain]` and `[toolchain.family_compilers]`, plus the generated file basename under `[codegen]`. The default output pair is `mcu_init.c` and `mcu_init.h`, but that basename remains configurable without changing the UI or generator code.

## Documentation

See [`docs/`](docs/) for current implementation details:

- [Architecture](docs/architecture.md): repo layout, runtime data flow, and module responsibilities
- [Tauri Commands](docs/commands.md): IPC contract between the frontend and Rust backend
- [Code Generation](docs/codegen.md): emitted files, init order, PPS/port handling, oscillator/fuse/CLC generation
- [Domain Knowledge](docs/domain.md): dsPIC33/PIC24 pin-routing concepts, fuses, oscillator behavior, overlays, and CLC notes
- [CLC](docs/clc.md): CLC data model, designer behavior, schematic routing, persistence, and backend code generation

## License

GNU General Public License v3.0 (GPLv3). See [LICENSE](LICENSE).
