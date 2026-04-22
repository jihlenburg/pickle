# TODO

## Active

- Verify that deleting `~/Library/Application Support/pickle` now resets the first-launch intro, since the dismissal state has moved from WebKit `localStorage` into `settings.toml`.
- Retry `Verify Pinout` for `DSPIC33AK256MPS205` after the prompt/schema contract fix and confirm the verifier again extracts the 48-pin package tables instead of returning zero packages.
- Verify in the Tauri app that applying a display-name-only shared package overlay now flips the verification view into the loaded/applied state instead of continuing to show it as a new package.
- Verify in the Tauri app that identical datasheet package tables now collapse onto one canonical selector entry, with shared labels such as `48-PIN VQFN/TQFP` replacing redundant built-in/overlay duplicates.
- Verify end-to-end that sibling dsPIC33AK parts now reuse the normalized verifier extraction cache and create entries under `verify_cache/` instead of paying for a second provider pass.
- Reload an AK MPS device after the backup-DCR parser fix and confirm `FDEVOPT` now shows `ALTI2C1/2/3` and `SPI2DIS`, while `FICD` exposes `BKBUG`.
- Verify in the Tauri app that dsPIC33AK parts without an `ICS` fuse no longer hard-reserve `PGC1`/`PGD1` as the active debug pair.
- Verify in the Tauri app that `ADxAN...` signals now produce ADC cards in the peripheral view instead of disappearing from the analog section.
- Verify in the Tauri app that the package selector now stays visible for single-package devices and that the attached package-actions menu cleanly replaces the old standalone `Edit Name...` button.
- Verify in the Tauri app that the left-panel `Pin View` / `Peripheral View` control now reads as real section tabs connected to the content pane instead of detached pill buttons.
- Verify in the Tauri app that dsPIC33AK devices with visible CLC endpoints no longer show the right-side `CLC` tab as disabled after a stale cache reload.
- Verify in the Tauri app that dsPIC33AK256MPS205 now exposes all 10 CLC modules in the CLC designer instead of the old fixed 4-tab limit, and that loading a saved config preserves higher-numbered CLC modules until the device reload completes.

## Backlog

- Lane A follow-ups: sweep remaining `title="…"` attrs in `frontend/index.html` (convert to `data-tip` or drop where redundant with visible labels); prune dead primitive classes (`.empty-state-action`, `.modal-lg`, `.stepper*`, `.checkbox/.switch`, `.btn-link`) and dead legacy `.key-row`/`.key-label`/`.key-field`/`.key-actions`.
- Lanes C (feedback & safety policy — toasts, progress, diff-before-apply), B (keyboard nav, undo/redo UI, ARIA polish), D (view density & clarity — Pin/Peripheral legends and filters) — Lane A is complete; these are next.
- Continue separating concerns in `src-tauri/src/parser/edc_parser.rs`.
- Add deeper dsPIC33AK-MPS-specific code generation support beyond the shared AK clock/CLC layer.
