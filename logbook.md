# Logbook

## 2026-04-22

- Lane A PR #8 polish: restored `.about-icon` keep-list rule (80×80 rounded + margin-bottom) so the About-dialog icon renders at the original size; hardened `PickleUI.modal` — idempotent `open(id)` (focusStack guards anchor + listener stacking), `confirm()` guards on missing `document`/`body` and labels the transient dialog via `aria-label`; dropped the `<span style="flex:1">` spacer in the package-dialog footer for `margin-left:auto` on Close; added 4 modal tests (idempotent open, missing-document confirm, DOM cleanup on close, settled-guard against double-resolve).
- Lane A close-out: final cross-PR review passed (READY_WITH_FOLLOWUP). Added `structure.test.js` guardrails that tokens.css loads first among component stylesheets and `00-namespace.js` loads first among `ui/*.js` helpers. Trimmed the done Lane A entry from `todo.md` and queued the deferred follow-ups (remaining `title="…"` sweep, dead primitive classes, dead legacy `.key-*` rows) under Backlog.

## 2026-04-21

- Lane A PR #8: modal primitive landed. All three dialogs (Package, About, Settings) migrated. window.confirm replaced with PickleUI.modal.confirm for delete-overlay. Lane A complete.
- Lane A PR #7: empty state primitive landed. Unified all four empties (info, fuses, CLC, verify).
- Lane A PR #6: tab strip primitive landed. Migrated three strips (right-panel, view toggle, CLC modules).
- Lane A PR #5: dropdown primitive landed. Migrated package menu, save menu, part picker. Removed ad-hoc menu markup and CSS.
- Lane A PR #5 polish: restored the `is-active` highlight on `PickleUI.select` by teaching the dropdown primitive to honor `item.active` and passing `items` as a factory from form.js (closure-captures `current`); guarded `refreshPartPickerSuggestions` with a same-list early return to stop part-picker flicker on catalog rebuilds while typing; refreshed dropdown.js header doc-comment to cover `meta?`, `active?`, and factory `items`; hardened the factory-rebuilds test with a `callCount` counter; added a smoke test that `dropdown(trigger)` with no opts tolerates an empty menu and Escape-close. Full suite: 92/92 green (12/12 dropdown + form).

## 2026-04-19

- Lane A PR #4: feedback-atoms primitive trio landed (tooltip, status bar, toast). Captured [title] + [data-tip], replaced keyword-sniffing with PickleUI.status(text, tone), added PickleUI.toast with stack limit 5.
- Lane A PR #4 polish: unified `.status-bar` CSS so the components/ primitive is no longer shadowed by 04-shell-layout.css, added a 10-toast hard cap for sticky-toast loops, and swept dead `[data-tone]` selectors plus stale tooltip comment and compile-check doc comment.
- Lane A PR #3: form primitive landed. Migrated verify-provider to PickleUI.select, swept inputs to .input.
- Lane A PR #2: button primitive landed. Migrated 21 button call sites; removed legacy .package-dialog-btn/.header-btn/.verify-btn/.key-{save,clear,reveal}/.about-{link,close} CSS.
- Lane A PR #1: scaffolding + tokens landed. Added components/tokens.css, ui/00-namespace.js, tests/ui/ folder; extended validate.sh.
- Frontend + Rust tests pass.
- Committed the CLC module tab-strip wrap change as `68d0c21` and pushed `main` to origin.
- Tagged the commit to snapshot the state before a model upgrade and pushed the tag.
- Brainstormed Lane A design-system-unification (frontend/). Settled on CSS-only primitives with small JS helpers under `window.PickleUI`; Comfortable (28 px) base density with Tight (22 px) tables; Flat-BEM naming; 8-PR staged migration (scaffolding+tokens → button → form → tooltip+status+toast → dropdown → tab strip → empty state → modal); status bar and toast primitives coexist with semantic APIs.
- Spec landed at `docs/superpowers/specs/2026-04-19-lane-a-design-system-unification-design.md`; implementation plan next.
- Added `.superpowers/` to `.gitignore` for brainstorming session artifacts (mockups + state).
- Lane A PR #2 follow-up: removed legacy ID-keyed button styling from 00-foundation.css and 01-pin-code.css so `.btn` tokens take effect (the PR #2 class-only sanity gate had missed these ID overrides).
- Lane A PR #3 follow-up: removed legacy `#part-input` ID styling from 00-foundation.css and 04-shell-layout.css (including the responsive width override in 05-peripheral-responsive.css) so `.input` tokens take effect, and expanded the sanity gate to cover this case.
- Lane A PR #3 polish: dropped `.btn` double-class from the provider select trigger, added outside-click and Escape close tests for `PickleUI.select`, and swept the now-dead `.key-input` HTML class plus the plan's Task 3.6 sanity-gate regex.

## 2026-04-09

- Wrapped the CLC module tab strip so devices with more than four modules, such as dsPIC33AK parts, keep all selector tabs visible after the device-driven module-count change.
- Re-ran the frontend Node test suite and `cargo test`; both passed after the CLC tab-strip follow-up.

## 2026-04-07

- Adopted repo-local `todo.md` and `logbook.md` tracking for `pickle`.
- Added a repo-local agent-instruction note so future work in this repository keeps those files updated.
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
