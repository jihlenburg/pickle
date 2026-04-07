# pickle

<p align="center">
  <img src="docs/pickle-icon-web.png" width="128" height="128" alt="pickle logo — a kawaii pickle with SO-8 metallic pins">
</p>

**pickle** is a native desktop pin configurator for Microchip dsPIC33 and PIC24 devices. It parses Microchip Device Family Pack (`.atpack`) data, renders package-aware pin assignment views, and generates compiler-friendly initialization code for PPS, port direction, oscillator, fuse, and CLC setup.

For newer dsPIC33AK parts, pickle now parses the 32-bit EDC naming model correctly for PPS and device inventory, and the generator emits the shared AK runtime clock-generator and CLC register sequences. Remaining AK gaps are now narrower: family-specific high-speed PWM, power, and other MPS-era peripheral init flows still need dedicated treatment instead of the older CK assumptions.

The name is a pun on **PIC** — Microchip's microcontroller line.

## What It Covers

- Interactive pin-table and peripheral-centric assignment views
- Package diagrams for DIP, SSOP, QFN, and QFP-style layouts
- PPS generation with explicit unlock/lock handling
- Port-mode generation for `ANSELx` and `TRISx`
- Optional oscillator pragma/init generation and dynamic fuse pragmas
- CLC designer with schematic/register preview and generated `CLCn*` writes
- Native open/save/export dialogs via Tauri
- Optional family-aware compile checks (`xc16-gcc` for PIC24, `xc-dsc-gcc` for dsPIC33) using installed or cached device packs, plus LLM-assisted datasheet verification
- Datasheet lookup re-validates cached local PDFs against the selected part or sibling family-series marker and skips obviously clipped extracts
- Pinout verification caches package-table extraction per datasheet PDF and pin-count scope, then computes diffs locally so sibling parts on the same family PDF can reuse the same extraction pass
- When a verified datasheet package table is pin-for-pin identical to an existing device-pack variant, pickle now collapses it onto that built-in package key and keeps the shared datasheet label as the UI-facing name instead of adding a redundant duplicate package
- The header now always shows the current package selector after a device loads, even when there is only one visible package, and a compact package-actions menu on that control handles rename/reset/delete flows
- Any package can get a local display-name override from the package-actions menu and dialog, while overlay-backed packages can also be deleted there after datasheet import
- First launch now shows a guided intro overlay with quick-start sample devices and a direct handoff into part search or config loading instead of a blank split-pane shell
- CLC availability is now inferred from the parsed device data itself instead of relying only on a cached module-ID hint, so stale dsPIC33AK caches with visible CLC endpoints no longer keep the CLC tab disabled

## Config Files

The app treats saved pin configurations as normal documents:

- `Save` writes back to the current path without reopening a dialog
- first save falls back to the native save dialog
- `Save As...` and `Rename...` are available from the split save menu and the native File menu
- the header shows the current config name plus an unsaved-changes indicator
- `Ctrl/Cmd+S` follows the same direct-save behavior as the toolbar/menu action

## Repo Layout

- `frontend/`: static HTML/CSS/JS loaded directly by the Tauri webview
- `frontend/static/app/`: ordered browser modules for state, pure policy/view/model helpers, document lifecycle, codegen/CLC workflows, shell actions, and verification model/render/orchestration
- `src-tauri/`: Rust backend, parser, split code generator helpers for orchestration/PPS/ports/CLC/single-file merge plus split oscillator backends, shared part-family profiles, settings, Tauri shell, split verification command helpers, and dedicated IPC type/support modules
- `src-tauri/src/parser/`: EDC/DFP loading plus split DFP root/datasheet/store helpers and verification-specific helpers for prompt scope, progress payloads, PDF reduction, provider dispatch/schema/transports, provider-stream normalization, extraction comparison, overlay persistence, and cache management
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

`settings.toml` also persists onboarding state under `[onboarding]`, compiler preferences under `[toolchain]` and `[toolchain.family_compilers]`, and the generated file basename under `[codegen]`. The default output pair is `mcu_init.c` and `mcu_init.h`, but that basename remains configurable without changing the UI or generator code.

On macOS you may still see `~/Library/WebKit/com.github.jihlenburg.pickle/` because the Tauri WebView creates its own browser-engine data directory. pickle should not rely on that for app-owned settings or credentials; intentional persistence lives in `settings.toml`, the OS keychain, and the runtime directories below.

On macOS the runtime tree normally looks like this:

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

Practical cleanup rules:

- `devices/`, `dfp_cache/edc/`, `dfp_cache/datasheets/`, and `dfp_cache/verify_cache/` are rebuildable caches.
- `pinouts/` and `clc_sources/` contain user-authored overlays and overrides; treat those as persistent data.
- Current builds write datasheet PDFs into `dfp_cache/datasheets/pdf/`. Older flat `dfp_cache/datasheets/<PART>.pdf` files are legacy cache artifacts and can be removed once the same part exists under `pdf/`.
- `dfp_cache/verify_cache/` stores extraction results keyed by PDF bytes plus a stable extraction scope such as pin-count or CLC extraction mode, not by the full selected part name.

[`docs/architecture.md`](docs/architecture.md) contains the full directory-by-directory reference, including which files are optional and what each cache is used for.

## Documentation

See [`docs/`](docs/) for current implementation details:

- [Architecture](docs/architecture.md): repo layout, runtime data flow, and module responsibilities
- [Tauri Commands](docs/commands.md): IPC contract between the frontend and Rust backend
- [Code Generation](docs/codegen.md): emitted files, init order, PPS/port handling, oscillator/fuse/CLC generation
- [Domain Knowledge](docs/domain.md): dsPIC33/PIC24 pin-routing concepts, fuses, oscillator behavior, overlays, and CLC notes
- [CLC](docs/clc.md): CLC data model, designer behavior, schematic routing, persistence, and backend code generation

## License

GNU General Public License v3.0 (GPLv3). See [LICENSE](LICENSE).

<!-- Keep this disclaimer block intact; tests/readme.test.js validates it. -->
<!-- mandatory-readme-legal-start -->
## Legal Disclaimer

pickle is an independent project and is not affiliated with, endorsed by, sponsored by, or approved by Microchip Technology Inc.

pickle is built to use publicly available technical information together with user-supplied or separately downloaded device data. The repository and application distribution do not include or redistribute Microchip-owned datasheets, device packs, images, or other source materials.

Microchip, dsPIC33, PIC24, and related product names, trademarks, logos, and brand names are the property of Microchip Technology Inc. All rights in that intellectual property remain with Microchip Technology Inc.
<!-- mandatory-readme-legal-end -->
