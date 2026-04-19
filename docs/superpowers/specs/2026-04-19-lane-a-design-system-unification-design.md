# Lane A — Design-System Unification

**Status:** Approved for planning (2026-04-19)
**Author:** Collaborative design session
**Scope:** Frontend only (`frontend/`); no Rust backend changes
**Follow-on lanes:** C (feedback & safety policy), B (interaction polish — keyboard nav), D (view density & clarity)

## 1. Purpose

Pickle's frontend UI has grown through a series of incremental additions. Dialogs, tab strips, dropdowns, empty states, tooltips, and status messaging each live in their own local styles, with per-feature class prefixes (`.package-dialog-*`, `.settings-*`, `.about-*`, `.verify-empty`, `.clc-empty`, `.device-info-empty`) and inconsistent shapes.

This spec lands a unified design-system layer: a single set of CSS primitives and small JS helpers that every screen draws from. Visual decisions (button height, radius, padding, font sizes, focus ring, colors) resolve to one canonical choice per primitive and propagate across the app.

**Success criteria:**
- All three existing dialogs share one modal primitive
- All three existing tab strips share one tab-strip primitive
- All three existing empty states share one empty-state primitive
- All three existing dropdowns (package actions, save menu, part-picker list) share one dropdown primitive
- Buttons, form controls, tooltips, toasts, and status bar converge to named primitives with a documented API
- Color tokens (`config.js`) are unchanged; new shape/size/motion tokens live in `components/tokens.css`
- The keyword-sniffing status-bar tone detection is replaced by a semantic `setStatus(text, tone)` API
- Native `title=` attributes and `[data-tip]` tooltips are unified under one tooltip system
- No regressions in existing keyboard shortcuts, theme toggle, welcome intro, Tauri menu actions

## 2. Design decisions

### 2.1 Shape: CSS-only primitives + small JS helpers

Primitives are defined as CSS classes in `frontend/static/styles/components/*.css`. Behavior (open/close, focus trap, dismiss, etc.) is provided by small JS helper modules in `frontend/static/app/ui/*.js`, attached to a `window.PickleUI` namespace.

Alternatives considered: JS factory functions that render markup, and custom web components. Rejected because they'd move dialog markup out of `index.html` (breaking the "all screens in one HTML file" property) and/or introduce a pattern the codebase hasn't used.

### 2.2 Scope: Maximal

All of the following are in scope for this spec:

- Button (variants: primary, secondary, ghost, danger, icon, small, link)
- Form controls (text input, select, numeric stepper, checkbox, switch, labeled-row)
- Modal (sizes sm/md/lg, with-nav variant, confirm variant)
- Tab strip (underline + segmented variants)
- Dropdown menu
- Tooltip (unifies `[data-tip]` + `title=`)
- Toast (tones info/success/warn/error/progress)
- Empty state
- Status bar (semantic tone API)

Feature-specific CSS (pin table, peripheral cards, CLC schematic, verification tables) is **not** in scope.

### 2.3 Visual intent: Light polish with maximum unification

Not a pure refactor: where current dialogs/empties/buttons differ, a canonical choice is picked and applied everywhere. No new visual language — same colors, typography, and feel as current pickle. Drift that exists today gets fixed in-line while the files are already open.

### 2.4 Density: Comfortable base + Tight tables

**Comfortable** is the default across buttons, form controls, modals, tab strips, dropdowns:
- 28 px control height
- 5 px border radius
- 10 / 14 px padding (footer / body)
- 12–13 px type

**Tight** is reserved for table row actions and in-table chrome (pin view, peripheral view, verification tables, register dumps):
- 22 px row height
- 3 px border radius
- 6 / 10 px padding
- 10–11 px type

### 2.5 Button vocabulary

Standard variants (28 px Comfortable): **primary** (accent bg), **secondary** (bordered transparent, also the default when no variant is applied), **ghost** (transparent, surfaces on hover), **danger** (error outlined by default, filled when combined with `.btn-primary`).

Compact variants: **icon** (28×28 square), **small** (22 px, for Tight table actions), **link** (inline accent text with underline, usually an `<a>`).

Unified focus ring on all variants: 2 px accent outline, 2 px offset.

### 2.6 Tab strip: Underline for 5-tab strips, Segmented for 2-way toggles

**Underline** style (2 px accent border under active tab, muted inactive) applies to:
- Right-panel tabs (Info / Fuses / CLC / Code / Verify)
- CLC module tabs inside the designer

**Segmented** style (connected outlined group, tinted active cell) applies to:
- Pin View / Peripheral View toggle

### 2.7 Modal anatomy

All modals share:
- Header: title (14 px semibold) + optional subtitle (11 px muted) + close X icon button
- Body: padded (18 px)
- Footer: right-aligned button row, **secondary left of primary**

Three size variants: `.modal-sm` (340 px), `.modal-md` (480 px), `.modal-lg` (640 px).
Plus a `.modal-with-nav` variant (680 px, two-column: left sidebar + content area) for Settings.
Plus a `.modal-confirm` convenience mode for small destructive confirmations (built on `.modal-sm` + a confirm helper).

### 2.8 Form controls

Text input, select (custom-styled — not native, consistent with pickle's earlier decision to drop native `<datalist>` for contrast), numeric stepper, checkbox, switch, and labeled-row (label + description + control, used in settings).

All share the same 28 px height, 5 px radius, and focus ring as buttons.

### 2.9 Feedback atoms

**Toast**: bottom-right stack, tone-colored left stripe (3 px), icon + title + body + optional action button, auto-dismiss after 5 s except `error` (manual) and `progress` (until explicitly updated/dismissed).

**Tooltip**: dark-surface popup (contrasts with dark app theme), 11 px type, 4 px radius, 4 px arrow. Auto-wires any `[data-tip]` or `[title]` on load; captured `title=` is stripped to prevent native-tooltip double-render.

**Empty state**: centered column inside a dashed border — icon (28 px, muted) + heading (13 px semibold) + description (11 px, max-width 40ch) + optional primary button as CTA. Replaces `.verify-empty`, `.clc-empty`, `.device-info-empty`.

**Status bar**: single persistent line in the app footer. Tones: `idle`, `busy` (adds spinner), `success`, `warn`, `error`. Accessed via `PickleUI.status(text, tone)`. Coexists with toasts — status bar reflects **current app state**, toasts convey **transient events**.

### 2.10 Dropdown menu

24 px tall items, icon column, optional divider, destructive item styled with error text at the bottom. One primitive covers package actions menu, save menu, and part-picker suggestion list.

### 2.11 Naming convention: Flat-BEM

`.modal`, `.modal-header`, `.modal-body`, `.modal-footer`, `.modal-wide`, `.btn`, `.btn-primary`, `.btn-danger`, `.tab-strip`, `.tab-strip-item`, etc. Hyphen for both children and modifiers; no double underscores. Consistent with pickle's existing style (`.package-dialog-header`, `.settings-section`), just standardized on the primitive name as the prefix.

## 3. Architecture & file layout

```
frontend/static/styles/
├── 00-foundation.css          (unchanged — reset, body, typography base)
├── components/                ← NEW; loaded between 00 and 01
│   ├── tokens.css
│   ├── button.css
│   ├── form.css
│   ├── modal.css
│   ├── tab-strip.css
│   ├── dropdown-menu.css
│   ├── tooltip.css
│   ├── toast.css
│   ├── empty-state.css
│   └── status-bar.css
├── 01-pin-code.css            (unchanged for Lane A)
├── 02-package-config.css      (shrinks as dialog and menu CSS extracts)
├── 03-verify-clc.css          (shrinks as tab-strip and empty-state extracts)
├── 04-shell-layout.css        (shrinks substantially; dialogs, tabs, status bar extract)
└── 05-peripheral-responsive.css (unchanged for Lane A)

frontend/static/app/
├── 00-core.js … 08-bootstrap.js  (existing numbered cascade)
└── ui/                        ← NEW; loaded after 00-core.js, before 01-reservation-policy.js
    ├── modal.js
    ├── toast.js
    ├── tooltip.js
    ├── dropdown.js
    ├── tab-strip.js
    ├── form.js                (hosts PickleUI.select — built on top of dropdown.js)
    └── status-bar.js

frontend/test/ui/              ← NEW; unit tests for the JS helpers
    ├── modal.test.js
    ├── toast.test.js
    ├── tooltip.test.js
    ├── dropdown.test.js
    ├── tab-strip.test.js
    ├── form.test.js
    └── status-bar.test.js
```

**Load order in `index.html`:** `00-foundation.css` → `components/tokens.css` → `components/*.css` (any order; the primitives are namespaced) → existing feature CSS (`01-*` through `05-*`). JS load order is the existing numbered cascade with `ui/*.js` inserted after `00-core.js` and before `01-reservation-policy.js`.

**Namespace:** All JS helpers attach to `window.PickleUI` (`PickleUI.modal.open(id)`, `PickleUI.toast(msg, opts)`, `PickleUI.status(text, tone)`), consistent with the existing `window.PickleConfig` pattern in `config.js`.

**Cascade guarantee:** `components/*.css` defines base primitive styles. Feature CSS only tweaks content, never shape. During migration, feature CSS that currently duplicates primitive rules gets deleted in the same PR as the primitive lands (no aliases or compatibility shims).

## 4. Design tokens (`components/tokens.css`)

Color tokens stay in `config.js` unchanged. The following additional scales are defined as CSS custom properties on `:root`:

```css
:root {
  /* Spacing */
  --space-1: 2px;   --space-6: 12px;
  --space-2: 4px;   --space-7: 14px;
  --space-3: 6px;   --space-8: 16px;
  --space-4: 8px;   --space-9: 20px;
  --space-5: 10px;  --space-10: 24px;

  /* Radius */
  --radius-sm: 3px;
  --radius-md: 5px;
  --radius-lg: 8px;
  --radius-full: 9999px;

  /* Control heights */
  --control-h-sm: 22px;
  --control-h-md: 28px;

  /* Font size */
  --text-xs: 10px;
  --text-sm: 11px;
  --text-md: 12px;
  --text-lg: 13px;
  --text-xl: 14px;

  /* Weight & leading */
  --weight-regular: 400;
  --weight-medium: 500;
  --weight-semibold: 600;
  --leading-tight: 1.3;
  --leading-normal: 1.5;

  /* Focus ring */
  --focus-ring-width: 2px;
  --focus-ring-offset: 2px;
  --focus-ring-color: var(--accent);

  /* Z-layer */
  --z-dropdown: 100;
  --z-sticky: 200;
  --z-overlay: 1000;
  --z-modal: 1010;
  --z-tooltip: 1500;
  --z-toast: 2000;

  /* Shadow */
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.1);
  --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.2);
  --shadow-lg: 0 8px 24px rgba(0, 0, 0, 0.25);

  /* Motion */
  --motion-fast: 150ms;
  --motion-medium: 250ms;
}
```

Tokens are plain CSS variables and cascade with the existing color tokens set by `config.js` on `:root`. No build step required.

## 5. Primitive catalog

### 5.1 Button — `components/button.css`

```
.btn                   base: height var(--control-h-md), radius var(--radius-md), text-md medium, focus ring
.btn-primary           background var(--accent), white text
.btn-secondary         border var(--border), transparent bg (default when no variant class applied)
.btn-ghost             transparent; hover → bg-tertiary
.btn-danger            error outlined; combine with .btn-primary for filled danger
.btn-icon              28×28 square, centered content
.btn-sm                height var(--control-h-sm) (22 px), radius-sm
.btn-link              inline accent text + underline (usually <a>)
```

States: `[disabled]`, `:hover`, `:focus-visible`. No JS.

**Replaces:** `.package-dialog-btn`, `.settings-save-btn`, `.settings-clear-btn`, ad-hoc header button styles.

### 5.2 Form controls — `components/form.css` + `ui/form.js`

```
.field                 wrapper for label + control + hint
.field-label           uppercase 10 px label
.field-hint            11 px muted, below control

.input                 base text input, 28 px, radius-md
.input-with-icon       flex row with leading glyph
.input-with-action     flex row with trailing button

.select                custom select wrapper
.select-trigger        28 px input-like button with trailing chevron
(popover uses .dropdown-menu primitive)

.stepper               inline-flex group
.stepper-btn           24 × 28 −/+ buttons with dividers
.stepper-input         centered monospaced value

.checkbox              16×16 square; checked state has accent bg + white check
.switch                28×16 pill + 12 px knob
.labeled-row           settings-style row (label + description + control)
```

JS (`ui/form.js`):
```js
PickleUI.select(trigger, {
  options: [{ value, label, icon? }],
  onSelect: (value) => void,
  placement?: 'top' | 'bottom',
})
```
Uses `PickleUI.dropdown` internally for the popover.

**Replaces:** ad-hoc inputs in Settings and Package dialog; native `<select>` where present (none now, but future-proof).

### 5.3 Modal — `components/modal.css` + `ui/modal.js`

Built on the native `<dialog>` element.

```
.modal                  <dialog> base: radius-lg, shadow-lg, centered, backdrop dim
.modal-sm / .modal-md / .modal-lg   widths 340 / 480 / 640 px
.modal-with-nav         680 px, two-column grid (nav | content)
.modal-confirm          applied programmatically via confirm() helper

.modal-header           flex row, padding space-7 space-8, border-bottom
.modal-title            text-xl semibold
.modal-subtitle         text-sm muted, optional
.modal-close            .btn-icon.btn-ghost, aria-label="Close"

.modal-body             padding space-8

.modal-footer           flex row, padding space-6 space-8, gap space-4,
                        justify-content flex-end, border-top, bg-primary

.modal-nav              left column inside .modal-with-nav, width 160, padding space-6 space-4, border-right
.modal-nav-item         24 px high, styled like .dropdown-item
.modal-nav-item.is-active
```

JS API:
```js
PickleUI.modal.open(id, { onClose? })        // id = element id of <dialog>
PickleUI.modal.close(id)
PickleUI.modal.confirm({
  title: string,
  message: string,
  action: string,             // label of primary button
  tone?: 'default' | 'danger'
}) → Promise<boolean>
```

Behavior: focus trap on open (saves previously focused element, restores on close), Esc closes, backdrop click closes (can be disabled per open call). The native `<dialog>` already manages its own focus and `::backdrop`; the helper layers focus-restoration and the confirm() abstraction on top.

**Replaces:** `.package-dialog-*`, `.settings-*`, `.about-*` class prefixes. Dialog markup in `index.html` stays but switches to the primitive's class names.

### 5.4 Tab strip — `components/tab-strip.css` + `ui/tab-strip.js`

```
.tab-strip                        underline default; flex row with bottom border
.tab-strip-item                   28 px cell, padding space-4 space-7, text-md, muted
.tab-strip-item.is-active         text-primary, weight-medium, 2 px accent border-bottom, margin-bottom -1 px

.tab-strip-segmented              inline-flex, border, radius-md, overflow hidden
.tab-strip-segmented .tab-strip-item                padding space-3 space-7, border-right
.tab-strip-segmented .tab-strip-item.is-active      bg-tertiary, text-primary weight-medium
```

JS API:
```js
PickleUI.tabStrip(container, {
  onChange: (id) => void,
})
```
Reads `[data-tab-id]` off each `.tab-strip-item`, applies `role=tablist/tab` + `aria-selected`, fires `onChange` on click. Keyboard navigation is deferred to Lane B.

**Replaces:** right-panel tab styles in `04-shell-layout.css`, view toggle in `02-view-model.js`, CLC module tab styles in `03-verify-clc.css`.

### 5.5 Dropdown menu — `components/dropdown-menu.css` + `ui/dropdown.js`

```
.dropdown-menu              positioned popover, shadow-md, radius-md, bg-secondary, border
.dropdown-item              24 px, flex row, padding 0 space-5, text-md
.dropdown-item-icon         text-sm, color var(--text-secondary), margin-right space-4
.dropdown-item.is-active    bg-tertiary
.dropdown-item.is-danger    color var(--error)
.dropdown-divider           1 px border, margin-y space-2
```

JS API:
```js
PickleUI.dropdown(trigger, {
  items: [{ id, label, icon?, danger?, divider? }],
  onSelect: (id) => void,
  placement?: 'bottom-start' | 'bottom-end' | 'top-start' | 'top-end',
})
```
Opens on trigger click, closes on click-outside or Esc. Keyboard nav deferred to Lane B.

**Replaces:** package actions menu in `06-shell.js`, save menu, part-picker suggestion list (the custom list that replaced native `<datalist>` in 2026-04-07).

### 5.6 Tooltip — `components/tooltip.css` + `ui/tooltip.js`

```
.tooltip                   position: fixed; inverted surface (background var(--text-primary), color var(--bg-primary)):
                           renders as a light popover in dark mode and a dark popover in light mode — contrasts with
                           the app regardless of theme;
                           padding space-3 space-5; radius-sm; shadow-md; text-sm; line-height 1.4; max-width 240 px
.tooltip-arrow             4 px triangle, same fill as surface
```

Behavior:
- On page load, JS scans for `[data-tip]` and `[title]` attributes, attaches hover/focus listeners.
- Captured `title=` is read into memory then stripped from the element (prevents native-tooltip double-render).
- 300 ms show delay; no dismiss delay on hover-out.
- Auto-flips above/below based on viewport edge distance.
- Single shared tooltip DOM node reused across all triggers.

**Replaces:** the existing custom `[data-tip]` logic in `08-bootstrap.js` and native `title=` attributes on buttons. One pass handles both.

### 5.7 Toast — `components/toast.css` + `ui/toast.js`

```
.toast-stack               position: fixed; bottom space-6, right space-6; flex column-reverse, gap space-5;
                           z-index var(--z-toast); pointer-events none (per-toast re-enables)
.toast                     bg-secondary, radius-md, shadow-md, border 1 px, border-left 3 px (tone color);
                           min-width 300 px, max-width 400 px; padding space-5 space-6; pointer-events auto
.toast-info                border-left color var(--text-secondary)
.toast-success             border-left color var(--success)
.toast-warn                border-left color var(--warning)
.toast-error               border-left color var(--error)
.toast-progress            border-left color var(--accent)
.toast-icon                text-xl leading glyph, tone color
.toast-title               text-md weight-medium
.toast-body                text-sm muted, margin-top space-1
.toast-action              .btn-sm slot
.toast-dismiss             .btn-icon.btn-ghost (18 × 18 in Tight context)
.toast-progress-bar        height 3 px, bg tertiary, inner accent fill at %progress
```

JS API:
```js
const t = PickleUI.toast(message, {
  tone?: 'info' | 'success' | 'warn' | 'error' | 'progress',    // default 'info'
  title?: string,                                                // defaults to tone label
  action?: { label, onClick },
  duration?: number,                                             // ms; default 5000
  sticky?: boolean,                                              // overrides duration
})
t.update({ title?, message?, progress? })   // progress toasts
t.dismiss()
```

Defaults:
- `info`, `success`, `warn`: auto-dismiss after 5 s
- `error`: sticky (manual dismiss required)
- `progress`: sticky (update or dismiss ends it)

Stack limit: max 5 visible toasts; when a 6th is pushed, the oldest auto-dismissible toast in the stack is removed to make room. `error` and `progress` toasts are not auto-evicted.

### 5.8 Empty state — `components/empty-state.css`

```
.empty-state             padding space-10 space-10, text-align center;
                         border 1 px dashed var(--border); radius-md
.empty-state-icon        font-size 28 px, color var(--text-tertiary), line-height 1, margin-bottom space-6
.empty-state-title       text-lg weight-semibold, margin-bottom space-2
.empty-state-body        text-sm muted, line-height 1.5, max-width 40ch, margin 0 auto space-7
.empty-state-action      slot for .btn-primary
```

No JS. **Replaces:** `.verify-empty`, `.clc-empty`, `.device-info-empty` (and any other empty-state divs).

### 5.9 Status bar — `components/status-bar.css` + `ui/status-bar.js`

```
.status-bar              existing footer; restyled to use tokens
.status-bar-tone-idle    color var(--text-secondary)
.status-bar-tone-busy    color var(--accent); ::before spinner (12 px, 2 px border, rotating)
.status-bar-tone-success color var(--success)
.status-bar-tone-warn    color var(--warning)
.status-bar-tone-error   color var(--error)
```

JS API:
```js
PickleUI.status(text, tone = 'idle')    // tone ∈ 'idle'|'busy'|'success'|'warn'|'error'
```

Replaces the keyword-sniffing in `00-core.js` (`tone = lower.includes('error') ? 'error' : ...`). Every existing caller of the status bar is updated to pass an explicit tone.

## 6. Migration sequence

Eight PRs, each self-contained. App remains fully functional between PRs.

### PR #1 — Scaffolding + tokens

- Add `frontend/static/styles/components/` folder with `tokens.css`
- Add `frontend/static/app/ui/` folder (empty placeholder + `window.PickleUI = {}` bootstrap)
- Wire `components/tokens.css` and (empty) `ui/` loads into `index.html`
- No visual change

### PR #2 — Button primitive

- Add `components/button.css`
- Migrate every button call site in `index.html` to `.btn` classes
- Delete per-dialog / per-header button CSS from `04-shell-layout.css`, `02-package-config.css`, `00-foundation.css`
- Sanity gate: `rg '\.(package-dialog|settings)-.*-btn' frontend/static/styles/` returns zero hits

### PR #3 — Form controls primitive

- Add `components/form.css` + `ui/form.js` (exposing `PickleUI.select`)
- Migrate Settings inputs, part picker input, package name input, any selects
- Delete corresponding feature CSS from `04-shell-layout.css` and `02-package-config.css`

Note: `ui/form.js` depends on `ui/dropdown.js` for the select popover. Because PR #5 lands the dropdown primitive later, PR #3 inlines a minimal popover implementation temporarily; PR #5 refactors `ui/form.js` to delegate to `PickleUI.dropdown`.

### PR #4 — Tooltip + Status bar + Toast

- Add `components/tooltip.css` + `ui/tooltip.js`; migrate all `[data-tip]` + `[title]` through it; remove the ad-hoc tooltip code from `08-bootstrap.js`; sweep `[title]` attributes off buttons
- Add `components/status-bar.css` + `ui/status-bar.js`; refactor every `setStatus` call site in `00-core.js`, `06-shell.js`, `07-verification*.js`, `08-bootstrap.js` to pass explicit tone
- Add `components/toast.css` + `ui/toast.js`; no feature-code hookup yet in this PR except: wire the existing "Saved" / compile-error signals through `PickleUI.toast` where they currently hit the status bar for transient events (status bar continues to own persistent state)

### PR #5 — Dropdown menu primitive

- Add `components/dropdown-menu.css` + `ui/dropdown.js`
- Migrate package actions menu, save menu, part-picker suggestion list
- Delete corresponding menu CSS from `02-package-config.css`, `04-shell-layout.css`

### PR #6 — Tab strip primitive

- Add `components/tab-strip.css` + `ui/tab-strip.js`
- Migrate right-panel tabs (underline), Pin/Peripheral toggle (segmented), CLC module tabs (underline)
- Delete corresponding tab CSS from `04-shell-layout.css` and `03-verify-clc.css`

### PR #7 — Empty state primitive

- Add `components/empty-state.css`
- Migrate `.verify-empty`, `.clc-empty`, `.device-info-empty` + any other empty-state divs
- Delete corresponding CSS from `03-verify-clc.css`, `04-shell-layout.css`, `07-verification-render.js` markup

### PR #8 — Modal primitive + dialog migration

- Add `components/modal.css` + `ui/modal.js` + `PickleUI.modal.confirm(...)`
- Migrate Package dialog, Settings dialog, About dialog to the new primitive
- Delete `.package-dialog-*`, `.settings-*`, `.about-*` legacy classes from `02-package-config.css`, `04-shell-layout.css`
- Final sanity gate: `rg '\.(package-dialog|settings-(section|nav|content)|about)-' frontend/static/styles/` returns zero hits

### Rules applying to every PR

- **No legacy aliases.** Rename and delete old classes in the same PR.
- **Net deletion.** After PR #1 and #2, every PR should delete more CSS than it adds.
- **Commit message:** `Lane A: <primitive name> primitive` + body listing legacy patterns removed.
- **Logbook:** Append a dated entry to `logbook.md` per merged PR.
- **Todo:** Update `todo.md` — mark PR items done, queue follow-up verification items.

### Fallback grouping

If 8 PRs is too granular, collapse to 4: (atoms) PRs #1 + #2 + #3 + #4-tooltip/status, (feedback) PRs #4-toast + #7 empty state, (navigation) PRs #5 + #6, (containers) PR #8. The recommended grouping is 8 for review tractability.

## 7. Testing & verification

### 7.1 New unit tests

Added in `frontend/test/ui/`:

| Helper | Tests |
|---|---|
| `modal.js` | open/close; focus trap entry + restore; Esc closes; backdrop click closes; `confirm()` resolves true/false |
| `toast.js` | stack ordering; auto-dismiss at 5 s; sticky (error, progress) stays; `update()` mutates DOM; `dismiss()` removes and cleans stack; max visible stack (e.g. 5) truncates oldest |
| `tooltip.js` | shows after 300 ms delay; flips up/down based on viewport; strips `title=` when capturing; `[data-tip]` takes precedence over `title=` |
| `dropdown.js` | opens on trigger click; click-outside closes; Esc closes; `onSelect` fires with item id; placement option respected |
| `tab-strip.js` | click activates; `aria-selected` toggled; `onChange` fires; initial active state from `[data-tab-id][data-active]` |
| `status-bar.js` | `setStatus(text, tone)` sets text; applies `status-bar-tone-<t>` class; removes previous tone class |
| `form.js` (`PickleUI.select`) | opens menu on trigger click; selects item; closes; fires callback |

Run via the existing `npm test` command in `frontend/`.

### 7.2 Manual verification per PR

Ticked in PR description:

- Launch `cargo tauri dev`; exercise every screen touched by the PR
- Dark ↔ light theme toggle renders correctly (token-driven)
- Keyboard shortcuts unaffected: ⌘S (save), ⌘Z / ⌘⇧Z (undo/redo)
- Fresh state (delete `~/Library/Application Support/pickle`) — welcome intro still appears and dismisses
- Tauri menu actions (File → Open, etc.) still wire through
- `npm test` in `frontend/` green
- `cargo test` green (Rust backend untouched; smoke check)

### 7.3 Regression hotspots

| PR | Risk area |
|---|---|
| #3 (Form) | Settings save/clear flow; part-picker input's custom suggestion list glue must keep working |
| #4 (Feedback) | Status bar visible on every screen; tone transitions (busy → success) must not flicker |
| #5 (Dropdown) | Part picker is main entry point — autocomplete must keep behaving during migration |
| #6 (Tab strip) | View switching heavily used; every active-tab callsite must be covered |
| #8 (Modal) | All three dialogs; native `<dialog>` focus-trap already exists — helper must not double-trap |

### 7.4 Ripgrep sanity gates

Run at the end of each PR:

```bash
# After PR #2 — no ad-hoc button styles
rg -l '\.(package-dialog|settings)-.*-btn' frontend/static/styles/

# After PR #4 — no native title= left on buttons covered by tooltips
rg 'title="[^"]+"' frontend/index.html | rg -v '<meta|<title>'

# After PR #7 — no legacy empty-state classes
rg -l '\.verify-empty|\.clc-empty|\.device-info-empty' frontend/

# After PR #8 — no legacy dialog classes
rg -l '\.(package-dialog|settings-(section|nav|content)|about)-' frontend/static/styles/
```

Any match fails the PR's self-check and must be fixed before merge.

### 7.5 Logbook entry format

Appended per merged PR:

```
## YYYY-MM-DD
- Lane A PR #N: <primitive> primitive landed. Migrated <call sites>. Removed <legacy classes>.
- Frontend + Rust tests pass.
```

## 8. Out of scope

Explicitly **not** in this spec (deferred to their respective lanes):

- **Lane C (feedback & safety policy):** deciding *when* to toast vs. status-bar-message; progress indicators on long-running operations beyond the primitive shape; diff-before-apply flow for destructive actions (delete overlay, config load overwrite)
- **Lane B (interaction polish):** keyboard navigation on custom dropdowns and tab strips (arrow keys, Home/End, Esc on submenus); undo/redo UI affordance; ARIA polish beyond the basics set up here
- **Lane D (view density & clarity):** Pin View / Peripheral View legends, active-filter indicators, reserved-pin muting; CLC designer layout improvements

The primitives created in Lane A are the structural foundation all three follow-on lanes bolt behavior onto.

## 9. Constraints & assumptions

- Tauri WebView; no new runtime dependencies added
- No frontend build step; plain JS + plain CSS (as today)
- Color tokens in `config.js` remain authoritative — new tokens in `components/tokens.css` only cover shape/size/motion
- All migration is mechanical: no new features, no re-architecting state
- Every PR must leave the app in a working, shippable state (no "in-progress" branches of primitives half-migrated)
