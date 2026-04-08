# Logbook

## 2026-04-07

- Adopted repo-local `todo.md` and `logbook.md` tracking for `pickle`.
- Added a repo-local `CLAUDE.md` note so future work in this repository keeps those files updated.
- Replaced the native part-number `datalist` popup with an app-rendered suggestion list to avoid the low-contrast WebView dropdown regression.
- Expanded `todo.md` to track the remaining UX, verification, parser, and dsPIC33AK follow-up work.
- Added a shell-owned first-launch intro overlay with config-driven copy, quick-start sample parts, and direct handoff into part search or config loading.
- Wired the startup bootstrap so the intro only appears for the empty first-run state and is dismissed once the user starts working or a device loads.
- Tightened frontend package identity dedupe so overlay-backed packages win even when equivalent labels differ only by whitespace or Unicode dash variants, and added a regression test for that case.
- Split verifier parsing one step further by introducing a normalized extraction model before local comparison, so the cache stores stable extraction JSON rather than depending on a second ad hoc raw-response parse.
- Canonicalized identical verified package tables onto the existing built-in package key, added overlay/dfp-store tests for that collapse path, and normalized shared datasheet labels like `48-PIN VQFN/TQFP (dsPIC...)` down to their useful UI form.
- Confirmed in the running Tauri app that the custom part-number picker fixed the low-contrast dropdown regression.
- Confirmed in the running Tauri app that the first-launch intro overlay flows feel right after real interaction.
- Moved the intro-dismissal state out of WebKit `localStorage` into `settings.toml`, removed the stale `localStorage` API-key read, and documented that remaining `~/Library/WebKit/com.github.jihlenburg.pickle/` data is browser-engine housekeeping rather than app-owned persistence.
- Fixed a verifier prompt/schema regression where the written pinout prompt still described object-keyed packages and string-keyed pin numbers while the structured provider schema required package arrays with integer `pin_number` fields; bumped the pinout extraction cache schema and skipped caching zero-package pinout results so the broken extraction contract would not be silently reused.
- Fixed the verification renderer so extracted package identities carry their `pin_count` even when the package name is not literally present in `device.packages`; this lets display-name-only overlay applications on canonical built-in packages register as already loaded/applied instead of remaining stuck in the "new package" state.
- Fixed AK fuse parsing so backup config DCRs like `FDEVOPTBKUP` merge back into their base register, exposed datasheet-visible `BKBUG` despite the pack tagging it hidden, and taught the cache freshness check to reparse stale AK JSON that is missing those backup-sector fuse fields.
- Fixed the frontend ICSP reservation fallback so devices without an `ICS` fuse no longer silently reserve `PGC1`/`PGD1` as if pair 1 were active; only explicit ICSP pair selections now hard-block a PGCx/PGDx pair.
- Fixed frontend ADC classification so dsPIC33AK-style `ADxAN...` pin functions are treated as ADC signals in analog detection, tag coloring, assignment grouping, and the peripheral-view instance builder.
- Reworked the header package control so the current package selector stays visible even for single-package devices, and moved rename/reset/delete actions into an attached package-actions menu instead of keeping a separate standalone `Edit Name...` button.
- Bumped the application version to `0.5.0` and updated the docs to describe the always-visible package selector plus attached package-actions menu.
- Restyled the left-panel `Pin View` / `Peripheral View` switcher so it behaves visually like connected section tabs tied to the content pane instead of detached floating pills.
- Synced the top-level `VERSION` file to `0.5.0` after the build script caught it lagging behind `Cargo.toml` and `tauri.conf.json`.
- Aligned the left-panel `Pin View` / `Peripheral View` styling with the right-panel tab strip so both sides now use the same underline-tab language.
- Fixed CLC capability detection so stale AK caches with visible `CLCINx` / `CLCxOUT` endpoints are reparsed and no longer keep the right-side CLC tab disabled just because `clc_module_id` was still `null`.
- Lifted the frontend CLC designer off its old hardcoded 4-module limit by deriving the module count from `device_info.clc` or visible `CLCxOUT` endpoints, updated the pure CLC model tests for higher-count devices, and preserved higher-numbered saved modules during config restore until the real device reload finishes.
