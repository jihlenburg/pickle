# Lane A — Design-System Unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract scattered per-feature CSS and ad-hoc JS into a unified set of design-system primitives (`window.PickleUI.*` + `frontend/static/styles/components/*.css`), migrate every consumer screen onto them, and delete the legacy class prefixes in the same PRs.

**Architecture:** CSS-only primitives in `frontend/static/styles/components/` plus small stateful helpers in `frontend/static/app/ui/` attached to a `window.PickleUI` namespace. No build step, no new runtime deps. Shape/size/motion tokens live in `components/tokens.css`; color tokens stay in `config.js`. Eight self-contained PRs; app remains shippable between each. Unit tests live in `frontend/tests/ui/` and run with `node:test`.

**Tech Stack:** Vanilla JS + plain CSS, Tauri WebView, `node:test`/`node:assert` for unit tests, `cargo test` + `scripts/validate.sh` for CI.

**Design spec:** `docs/superpowers/specs/2026-04-19-lane-a-design-system-unification-design.md`

**Path note:** The spec references `frontend/test/ui/`, but the actual repo uses `frontend/tests/` (validated by `scripts/validate.sh`). This plan uses `frontend/tests/ui/` to match the repo and extends `scripts/validate.sh` to pick up the new subdirectory in Task 1.5.

---

## PR #1 — Scaffolding + tokens

Creates directory structure, design tokens, the empty `window.PickleUI` namespace, wires everything into load order, and extends `validate.sh` to pick up the new test subfolder. No visual change; no consumer migration.

### Task 1.1: Create component/ui/test directory structure

**Files:**
- Create: `frontend/static/styles/components/.gitkeep`
- Create: `frontend/static/app/ui/.gitkeep`
- Create: `frontend/tests/ui/.gitkeep`

- [ ] **Step 1: Create directories**

```bash
mkdir -p frontend/static/styles/components frontend/static/app/ui frontend/tests/ui
touch frontend/static/styles/components/.gitkeep frontend/static/app/ui/.gitkeep frontend/tests/ui/.gitkeep
```

### Task 1.2: Add design tokens

**Files:**
- Create: `frontend/static/styles/components/tokens.css`

- [ ] **Step 1: Write `tokens.css`**

```css
/*
 * Lane A design-system tokens.
 *
 * Shape, size, and motion scales used by every primitive in
 * frontend/static/styles/components/*.css. Color tokens live in
 * frontend/static/app/config.js and cascade independently on :root.
 */
:root {
    /* Spacing */
    --space-1: 2px;
    --space-2: 4px;
    --space-3: 6px;
    --space-4: 8px;
    --space-5: 10px;
    --space-6: 12px;
    --space-7: 14px;
    --space-8: 16px;
    --space-9: 20px;
    --space-10: 24px;

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

### Task 1.3: Create the `PickleUI` namespace bootstrap

**Files:**
- Create: `frontend/static/app/ui/00-namespace.js`
- Create: `frontend/tests/ui/namespace.test.js`

- [ ] **Step 1: Write the failing test**

```javascript
// frontend/tests/ui/namespace.test.js
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const scriptPath = path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js');

test('ui/00-namespace.js creates window.PickleUI', () => {
    const source = fs.readFileSync(scriptPath, 'utf8');
    const sandbox = { window: {} };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);
    assert.equal(typeof sandbox.window.PickleUI, 'object');
});

test('ui/00-namespace.js does not overwrite an existing PickleUI', () => {
    const source = fs.readFileSync(scriptPath, 'utf8');
    const existing = { preserved: true };
    const sandbox = { window: { PickleUI: existing } };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);
    assert.equal(sandbox.window.PickleUI, existing);
    assert.equal(sandbox.window.PickleUI.preserved, true);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `node --test frontend/tests/ui/namespace.test.js`
Expected: FAIL (file does not exist yet).

- [ ] **Step 3: Write the namespace bootstrap**

```javascript
// frontend/static/app/ui/00-namespace.js
/*
 * PickleUI namespace.
 *
 * Top-level host for design-system helpers (modal, toast, tooltip,
 * dropdown, tab-strip, form, status-bar). Every ui/*.js file attaches
 * its exports under window.PickleUI and must not overwrite siblings.
 */
(function initPickleUI(global) {
    if (!global.PickleUI || typeof global.PickleUI !== 'object') {
        global.PickleUI = {};
    }
})(typeof window !== 'undefined' ? window : globalThis);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `node --test frontend/tests/ui/namespace.test.js`
Expected: PASS.

### Task 1.4: Wire `tokens.css` and namespace script into `index.html` and `style.css`

**Files:**
- Modify: `frontend/static/style.css`
- Modify: `frontend/index.html`

- [ ] **Step 1: Add tokens import to `style.css`**

Insert the `components/tokens.css` import immediately after the `00-foundation.css` line:

```css
@import url("styles/00-foundation.css");
@import url("styles/components/tokens.css");
@import url("styles/01-pin-code.css");
@import url("styles/02-package-config.css");
@import url("styles/03-verify-clc.css");
@import url("styles/04-shell-layout.css");
@import url("styles/05-peripheral-responsive.css");
```

- [ ] **Step 2: Add the namespace script to `index.html`**

Insert the namespace `<script>` between `model.js` and `00-core.js`:

```html
<script src="static/pin_descriptions.js"></script>
<script src="static/app/model.js"></script>
<script src="static/app/ui/00-namespace.js"></script>
<script src="static/app/00-core.js"></script>
```

### Task 1.5: Extend `scripts/validate.sh` to cover `ui/` tests and scripts

**Files:**
- Modify: `scripts/validate.sh`

- [ ] **Step 1: Update the validate script**

Change the existing JS syntax check loop and test glob from flat to covering the `ui/` subdirectory:

```bash
node --check frontend/static/pin_descriptions.js
for file in frontend/static/app/*.js frontend/static/app/ui/*.js; do
    node --check "$file"
done

node --test frontend/tests/*.test.js frontend/tests/ui/*.test.js tests/*.test.js
```

- [ ] **Step 2: Run validate**

Run: `scripts/validate.sh`
Expected: PASS (Rust + frontend tests both green).

### Task 1.6: Commit PR #1

- [ ] **Step 1: Stage and commit**

```bash
git add frontend/static/styles/components/ frontend/static/app/ui/ frontend/tests/ui/ frontend/static/style.css frontend/index.html scripts/validate.sh
git commit -m "$(cat <<'EOF'
Lane A: scaffolding + design tokens

- Add frontend/static/styles/components/ with tokens.css (shape/size/motion)
- Add frontend/static/app/ui/ with 00-namespace.js bootstrapping window.PickleUI
- Add frontend/tests/ui/ with namespace.test.js coverage
- Wire tokens.css into style.css and ui/00-namespace.js into index.html
- Extend scripts/validate.sh to pick up ui/ scripts and tests
EOF
)"
```

- [ ] **Step 2: Append logbook entry**

Append under today's date in `logbook.md`:

```
- Lane A PR #1: scaffolding + tokens landed. Added components/tokens.css, ui/00-namespace.js, tests/ui/ folder; extended validate.sh.
- Frontend + Rust tests pass.
```

- [ ] **Step 3: Update todo.md**

Mark the PR #1 bullet done (once the todo.md is updated to track PR-level items) and queue PR #2 verification.

---

## PR #2 — Button primitive

Adds the shared button CSS, migrates every button call site in `index.html` to the new classes, deletes the legacy prefixes.

### Task 2.1: Write `components/button.css`

**Files:**
- Create: `frontend/static/styles/components/button.css`
- Modify: `frontend/static/app/config.js`
- Modify: `frontend/static/style.css`

- [ ] **Step 1: Add `--error` and `--on-accent` color tokens to `config.js`**

PR #1's `components/tokens.css` holds shape/size/motion tokens only — color tokens live in `frontend/static/app/config.js` under the `themes.dark` and `themes.light` blocks (see the comment at the top of `tokens.css`). The button primitive needs two color tokens that don't exist yet:

- `--error` — danger foreground / background. Must stay visually distinct from `--accent` (which is `#ff6b6b` / `#d9485f` in pickle, already a red) so `.btn-primary` and `.btn-danger` don't collapse to the same color.
- `--on-accent` — readable text on an `--accent` background. Pickle's accents are saturated enough that pure white reads cleanly in both themes.

In `frontend/static/app/config.js`, add to the `dark` theme block (place after the existing `--text-inverse` line around line 212):

```js
'--error': '#ef4444',
'--on-accent': '#ffffff',
```

Add to the `light` theme block with the dark-variant for `--error` (place after the existing `--text-inverse` line around line 269):

```js
'--error': '#dc2626',
'--on-accent': '#ffffff',
```

Keep the existing alphabetical-ish ordering the file uses — near `--text-inverse` / `--accent` is fine.

- [ ] **Step 2: Write the button stylesheet**

```css
/*
 * Button primitive.
 *
 * .btn is the base; variants add color/shape. Height and radius come from
 * tokens; focus ring is 2 px accent outline with 2 px offset on every
 * variant. No JS — behavior is pure CSS.
 */
.btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    height: var(--control-h-md);
    padding: 0 var(--space-6);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: transparent;
    color: var(--text);
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    line-height: 1;
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
    transition: background-color var(--motion-fast) ease,
                border-color var(--motion-fast) ease,
                color var(--motion-fast) ease;
}

.btn:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring-color);
    outline-offset: var(--focus-ring-offset);
}

.btn[disabled],
.btn.is-disabled {
    opacity: 0.5;
    cursor: not-allowed;
    pointer-events: none;
}

.btn-primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--on-accent);
}

.btn-primary:hover {
    filter: brightness(1.08);
}

.btn-secondary {
    background: transparent;
    border-color: var(--border);
    color: var(--text);
}

.btn-secondary:hover {
    background: var(--hover-overlay);
}

.btn-ghost {
    border-color: transparent;
    background: transparent;
    color: var(--text);
}

.btn-ghost:hover {
    background: var(--hover-overlay);
}

.btn-danger {
    border-color: var(--error);
    color: var(--error);
    background: transparent;
}

.btn-danger:hover {
    background: color-mix(in srgb, var(--error) 12%, transparent);
}

.btn-danger.btn-primary {
    background: var(--error);
    color: var(--on-accent);
    border-color: var(--error);
}

.btn-icon {
    width: var(--control-h-md);
    height: var(--control-h-md);
    padding: 0;
}

.btn-sm {
    height: var(--control-h-sm);
    padding: 0 var(--space-5);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
}

.btn-sm.btn-icon {
    width: var(--control-h-sm);
    padding: 0;
}

.btn-link {
    height: auto;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--accent);
    text-decoration: underline;
    font-weight: var(--weight-regular);
}

.btn-link:hover {
    filter: brightness(1.15);
}
```

- [ ] **Step 3: Import `button.css` from `style.css`**

After the `components/tokens.css` line add:

```css
@import url("styles/components/button.css");
```

### Task 2.2: Migrate header + toolbar buttons in `index.html`

**Files:**
- Modify: `frontend/index.html`

- [ ] **Step 1: Migrate header action buttons**

Convert each header/toolbar button to use `.btn` + the appropriate variant:

```html
<!-- before: <button id="load-btn">Load</button> -->
<button id="load-btn" class="btn btn-primary">Load</button>

<!-- before: <button id="verify-btn" class="verify-btn" ...>Verify Pinout</button> -->
<button id="verify-btn" class="btn btn-secondary" style="display:none"
        title="Cross-check pinout against the datasheet using the configured verifier">Verify Pinout</button>

<!-- before: <button id="load-btn-file">Open Config</button> -->
<button id="load-btn-file" class="btn btn-secondary">Open Config</button>

<!-- before: <button id="save-btn" ...>Save Config</button> -->
<button id="save-btn" class="btn btn-primary" title="Save current configuration (Ctrl+S)">Save Config</button>

<!-- before: <button id="save-menu-btn" type="button" class="save-menu-btn" ...>▾</button> -->
<button id="save-menu-btn" type="button" class="btn btn-icon btn-primary save-menu-btn"
        aria-haspopup="menu" aria-expanded="false" title="More save actions">▾</button>

<!-- before: <button id="settings-btn" type="button" class="header-btn" ...>Settings</button> -->
<button id="settings-btn" type="button" class="btn btn-ghost"
        title="Settings (Cmd+,)">Settings</button>

<!-- before: <button id="theme-toggle">System</button> -->
<button id="theme-toggle" class="btn btn-secondary">System</button>

<!-- before: <button id="pkg-menu-btn" ... class="package-menu-btn" ...>...</button> -->
<button id="pkg-menu-btn" type="button" class="btn btn-icon btn-ghost package-menu-btn"
        aria-haspopup="menu" aria-expanded="false" aria-label="Package actions"
        title="Package actions">...</button>
```

Keep the per-feature classes (`save-menu-btn`, `package-menu-btn`) as hooks for JS; they will be removed in PR #5 when the menus migrate to `PickleUI.dropdown`.

- [ ] **Step 2: Migrate code-tab toolbar buttons**

```html
<button id="gen-btn" class="btn btn-primary">Generate C Code</button>
<button id="check-btn" class="btn btn-secondary" style="display:none">Compiler Check</button>
<button id="copy-btn" class="btn btn-secondary">Copy</button>
<button id="export-btn" class="btn btn-secondary">Export Files</button>
```

- [ ] **Step 3: Migrate summary + pin-list buttons**

```html
<!-- before: <button id="pinlist-btn" class="summary-btn">Save Pin List</button> -->
<button id="pinlist-btn" class="btn btn-sm btn-secondary">Save Pin List</button>

<!-- before: <button id="index-badge" ... class="index-badge" ...></button> -->
<button id="index-badge" type="button" class="btn btn-sm btn-ghost index-badge"
        style="display:none" title="Click to refresh"></button>
```

### Task 2.3: Migrate package dialog buttons

**Files:**
- Modify: `frontend/index.html`

- [ ] **Step 1: Replace `.package-dialog-btn` / close / footer buttons**

```html
<!-- close X -->
<button id="package-close-btn" type="button"
        class="btn btn-icon btn-ghost package-dialog-close"
        aria-label="Close package dialog">Close</button>

<!-- footer -->
<button id="package-delete-btn" type="button" class="btn btn-danger">Delete Overlay</button>
<button id="package-cancel-btn" type="button" class="btn btn-secondary">Close</button>
<button id="package-reset-btn" type="button" class="btn btn-secondary">Reset Name</button>
<button id="package-save-btn" type="button" class="btn btn-primary">Save Name</button>
```

### Task 2.4: Migrate About + Settings dialog buttons

**Files:**
- Modify: `frontend/index.html`

- [ ] **Step 1: About dialog buttons**

```html
<button class="btn btn-secondary about-link" id="about-github-btn">GitHub</button>

<button class="btn btn-primary about-close" id="about-close-btn">Close</button>
```

- [ ] **Step 2: Settings dialog buttons**

Replace every `.key-save` / `.key-clear` / `.key-reveal` / `.settings-close` / `.settings-nav-btn` button with `.btn` variants:

```html
<button class="btn btn-icon btn-ghost key-reveal" id="key-reveal-openai"
        title="Show / hide key">&#x1f441;</button>

<button class="btn btn-sm btn-primary key-save" id="key-save-openai">Save</button>
<button class="btn btn-sm btn-secondary key-clear" id="key-clear-openai">Clear</button>

<!-- (same pattern for -anthropic siblings) -->

<button class="btn btn-ghost settings-nav-btn active"
        data-section="api-keys">API Keys</button>

<button class="btn btn-primary settings-close" id="settings-close-btn">Done</button>
```

Note: `settings-nav-btn` and the `active` class are temporary hooks retained for the existing JS in `05-settings.js` (which toggles `.active`); they are deleted in PR #8 when modal nav migrates to the modal primitive's `.modal-nav` with `.is-active`.

### Task 2.5: Migrate package-select package split + view toggle buttons

**Files:**
- Modify: `frontend/index.html`

- [ ] **Step 1: View toggle buttons**

Retain the `view-toggle-btn` / `active` hook classes (PR #6 replaces them with `.tab-strip-item`):

```html
<button class="btn btn-sm btn-ghost view-toggle-btn active" data-view="pin">Pin View</button>
<button class="btn btn-sm btn-ghost view-toggle-btn" data-view="peripheral">Peripheral View</button>
```

- [ ] **Step 2: Package menu items**

Items inside `#pkg-menu` become ghost small buttons; they'll get full `.dropdown-item` treatment in PR #5:

```html
<button id="pkg-edit-name-btn" type="button" class="btn btn-ghost btn-sm package-menu-item">Edit Name...</button>
<button id="pkg-reset-name-btn" type="button" class="btn btn-ghost btn-sm package-menu-item">Reset Name</button>
<button id="pkg-delete-btn" type="button" class="btn btn-ghost btn-sm package-menu-item">Delete Overlay</button>
```

Apply the same treatment to the save-menu items (`#save-as-btn`, `#rename-btn`).

### Task 2.6: Delete legacy button CSS

**Files:**
- Modify: `frontend/static/styles/02-package-config.css`
- Modify: `frontend/static/styles/04-shell-layout.css`
- Modify: `frontend/static/styles/00-foundation.css`

- [ ] **Step 1: Remove button rules from `02-package-config.css`**

Delete rules matching `.package-dialog-btn`, `.package-dialog-close`, `.package-delete-btn`, `.package-cancel-btn`, `.package-reset-btn`, `.package-save-btn`, `.package-menu-btn`. Keep only the rules that govern non-button aspects (positioning, container layout) if any.

- [ ] **Step 2: Remove button rules from `04-shell-layout.css`**

Delete `.header-btn`, `.verify-btn`, button-specific styles from `#load-btn`, `#load-btn-file`, `#save-btn`, `#save-menu-btn`, `#gen-btn`, `#check-btn`, `#copy-btn`, `#export-btn`, `#theme-toggle`, `#index-badge`, `.summary-btn`, `.key-save`, `.key-clear`, `.key-reveal`, `.settings-close`, `.settings-nav-btn`, `.about-link`, `.about-close`, `.save-menu-btn`.

Keep positional/layout rules (e.g. `.save-split` flex layout, `.key-actions` flex gaps).

- [ ] **Step 3: Remove stray button resets from `00-foundation.css`**

If a generic `button { ... }` rule exists that sets global padding/border/background, replace it with only the reset needed (eg. `font: inherit`) — all styling now lives in `.btn`.

### Task 2.7: Sanity gate — no legacy button class rules survive

- [ ] **Step 1: Run the grep gate**

Run:
```bash
rg '^\s*\.(package-dialog-btn|package-dialog-close|package-delete-btn|package-cancel-btn|package-reset-btn|package-save-btn|header-btn|verify-btn|settings-close|settings-nav-btn|key-save|key-clear|key-reveal|about-link|about-close|save-menu-btn|summary-btn)\b' frontend/static/styles/
```
Expected: zero matches. If any match, the CSS rule must be deleted (it's covered by `.btn*` now).

- [ ] **Step 2: Run the full validation**

Run: `scripts/validate.sh`
Expected: PASS.

- [ ] **Step 3: Launch and sanity-check the app**

Run: `cargo tauri dev`
Verify visually: header buttons render, Load/Verify/Save row looks right in both dark and light themes, package dialog footer is readable, Settings save/clear work, About dialog closes correctly.

### Task 2.8: Commit PR #2

- [ ] **Step 1: Stage and commit**

```bash
git add frontend/static/app/config.js frontend/static/styles/components/button.css frontend/static/style.css frontend/index.html frontend/static/styles/00-foundation.css frontend/static/styles/02-package-config.css frontend/static/styles/04-shell-layout.css
git commit -m "$(cat <<'EOF'
Lane A: button primitive

- Add --error and --on-accent color tokens to both themes in config.js
- Add components/button.css (primary/secondary/ghost/danger/icon/sm/link + focus ring)
- Migrate every button in index.html to .btn classes with appropriate variants
- Delete legacy button CSS from 00-foundation, 02-package-config, 04-shell-layout
- Sanity gate: rg ensures no .*-btn class-rule leftovers
EOF
)"
```

- [ ] **Step 2: Append logbook entry**

Under today's date add: `Lane A PR #2: button primitive landed. Migrated ~20 button call sites; removed legacy .package-dialog-btn/.header-btn/.verify-btn/.key-{save,clear,reveal}/.about-{link,close} CSS.`

---

## PR #3 — Form controls primitive

Adds `components/form.css` and `ui/form.js` (exposing `PickleUI.select`). Migrates Settings inputs, Package dialog input, and Verify-provider select. PR #3 ships a minimal inline popover in `ui/form.js`; PR #5 refactors it to delegate to `PickleUI.dropdown`.

### Task 3.1: Write `components/form.css`

**Files:**
- Create: `frontend/static/styles/components/form.css`
- Modify: `frontend/static/style.css`

- [ ] **Step 1: Write form stylesheet**

```css
/*
 * Form controls primitive.
 *
 * Text input, custom select trigger, numeric stepper, checkbox, switch,
 * and labeled-row. Control height matches buttons (28 px). Native
 * <select> is avoided; use PickleUI.select + .select-trigger instead.
 */
.field {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
}

.field-label {
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-dim);
}

.field-hint {
    font-size: var(--text-sm);
    color: var(--text-dim);
    line-height: var(--leading-normal);
}

.input {
    display: block;
    width: 100%;
    height: var(--control-h-md);
    padding: 0 var(--space-5);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-card);
    color: var(--text);
    font-size: var(--text-md);
    font-family: inherit;
    line-height: 1;
    transition: border-color var(--motion-fast) ease;
}

.input:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring-color);
    outline-offset: var(--focus-ring-offset);
    border-color: var(--accent);
}

.input[disabled] {
    opacity: 0.6;
    cursor: not-allowed;
}

.input-with-icon,
.input-with-action {
    display: flex;
    align-items: stretch;
    gap: var(--space-2);
}

.input-with-icon .input,
.input-with-action .input {
    flex: 1;
}

.select {
    position: relative;
    display: inline-flex;
}

.select-trigger {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    height: var(--control-h-md);
    padding: 0 var(--space-5);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-card);
    color: var(--text);
    font-size: var(--text-md);
    font-family: inherit;
    cursor: pointer;
    min-width: 140px;
}

.select-trigger::after {
    content: "▾";
    font-size: var(--text-sm);
    color: var(--text-dim);
}

.select-trigger:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring-color);
    outline-offset: var(--focus-ring-offset);
}

.select-trigger[aria-expanded="true"] {
    border-color: var(--accent);
}

.stepper {
    display: inline-flex;
    align-items: stretch;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
    height: var(--control-h-md);
}

.stepper-btn {
    width: 24px;
    border: 0;
    background: var(--bg-card);
    color: var(--text);
    font-size: var(--text-md);
    cursor: pointer;
}

.stepper-btn:hover {
    background: var(--hover-overlay);
}

.stepper-btn + .stepper-input {
    border-left: 1px solid var(--border);
    border-right: 1px solid var(--border);
}

.stepper-input {
    width: 60px;
    border: 0;
    text-align: center;
    font-family: ui-monospace, monospace;
    font-size: var(--text-md);
    background: transparent;
    color: var(--text);
}

.stepper-input:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring-color);
    outline-offset: -1px;
}

.checkbox {
    appearance: none;
    width: 16px;
    height: 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-card);
    cursor: pointer;
    position: relative;
    flex-shrink: 0;
}

.checkbox:checked {
    background: var(--accent);
    border-color: var(--accent);
}

.checkbox:checked::after {
    content: "";
    position: absolute;
    left: 4px;
    top: 1px;
    width: 5px;
    height: 9px;
    border-right: 2px solid var(--on-accent);
    border-bottom: 2px solid var(--on-accent);
    transform: rotate(45deg);
}

.checkbox:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring-color);
    outline-offset: var(--focus-ring-offset);
}

.switch {
    appearance: none;
    width: 28px;
    height: 16px;
    border-radius: var(--radius-full);
    background: var(--hover-overlay);
    border: 1px solid var(--border);
    position: relative;
    cursor: pointer;
    flex-shrink: 0;
    transition: background-color var(--motion-fast) ease;
}

.switch::before {
    content: "";
    position: absolute;
    left: 1px;
    top: 1px;
    width: 12px;
    height: 12px;
    border-radius: var(--radius-full);
    background: var(--text);
    transition: transform var(--motion-fast) ease;
}

.switch:checked {
    background: var(--accent);
    border-color: var(--accent);
}

.switch:checked::before {
    transform: translateX(12px);
    background: var(--on-accent);
}

.switch:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring-color);
    outline-offset: var(--focus-ring-offset);
}

.labeled-row {
    display: grid;
    grid-template-columns: minmax(120px, 30%) 1fr auto;
    gap: var(--space-6);
    align-items: center;
    padding: var(--space-4) 0;
    border-bottom: 1px solid var(--border);
}

.labeled-row:last-child {
    border-bottom: 0;
}

.labeled-row-label {
    font-size: var(--text-md);
    color: var(--text);
    font-weight: var(--weight-medium);
}

.labeled-row-desc {
    font-size: var(--text-sm);
    color: var(--text-dim);
    line-height: var(--leading-normal);
}
```

- [ ] **Step 2: Import it**

Add to `style.css` after `button.css`:
```css
@import url("styles/components/form.css");
```

### Task 3.2: Write unit test for `PickleUI.select`

**Files:**
- Create: `frontend/tests/ui/form.test.js`

- [ ] **Step 1: Write the failing test**

```javascript
// frontend/tests/ui/form.test.js
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function loadForm() {
    const namespace = fs.readFileSync(
        path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const form = fs.readFileSync(
        path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'form.js'), 'utf8');
    return namespace + '\n' + form;
}

function makeDom() {
    const listeners = new Map();
    const doc = {
        _els: [],
        body: { _children: [], appendChild(el) { this._children.push(el); } },
        addEventListener(type, fn) {
            (listeners.get(type) || listeners.set(type, []).get(type)).push(fn);
        },
        removeEventListener(type, fn) {
            const arr = listeners.get(type) || [];
            const idx = arr.indexOf(fn);
            if (idx !== -1) arr.splice(idx, 1);
        },
        dispatch(type, event) {
            for (const fn of (listeners.get(type) || [])) fn(event);
        },
        createElement(tag) {
            const el = {
                tagName: tag.toUpperCase(),
                children: [],
                classList: new Set(),
                dataset: {},
                style: {},
                attributes: {},
                textContent: '',
                setAttribute(n, v) { this.attributes[n] = v; },
                removeAttribute(n) { delete this.attributes[n]; },
                getAttribute(n) { return this.attributes[n]; },
                appendChild(c) { this.children.push(c); c.parentNode = this; return c; },
                remove() {
                    if (this.parentNode) {
                        const i = this.parentNode.children.indexOf(this);
                        if (i !== -1) this.parentNode.children.splice(i, 1);
                    }
                },
                addEventListener(t, f) { (this._l ||= {})[t] = ((this._l?.[t]) || []).concat(f); },
                click() { for (const f of (this._l?.click || [])) f({ target: this, stopPropagation() {}, preventDefault() {} }); },
                getBoundingClientRect() { return { left: 0, top: 0, right: 100, bottom: 28, width: 100, height: 28 }; },
                contains(other) { return other === this || this.children.some(c => c.contains && c.contains(other)); },
            };
            el.classList.add = (c) => el.classList instanceof Set ? Set.prototype.add.call(el.classList, c) : null;
            doc._els.push(el);
            return el;
        },
    };
    // Fix classList to behave like DOMTokenList
    const origCreate = doc.createElement;
    doc.createElement = function(tag) {
        const el = origCreate.call(this, tag);
        const set = new Set();
        el.classList = {
            add: (c) => set.add(c),
            remove: (c) => set.delete(c),
            toggle: (c, force) => {
                const has = set.has(c);
                if (force === true || (force === undefined && !has)) set.add(c);
                else set.delete(c);
            },
            contains: (c) => set.has(c),
            _set: set,
        };
        return el;
    };
    return doc;
}

test('PickleUI.select renders options and fires onSelect with value', () => {
    const source = loadForm();
    const document = makeDom();
    const trigger = document.createElement('button');
    const sandbox = { window: {}, document };
    sandbox.window.document = document;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const received = [];
    sandbox.window.PickleUI.select(trigger, {
        options: [{ value: 'a', label: 'Alpha' }, { value: 'b', label: 'Beta' }],
        onSelect: (v) => received.push(v),
    });

    // Clicking the trigger opens the menu (appended to document.body).
    trigger.click();
    const menu = document.body._children[document.body._children.length - 1];
    assert.ok(menu, 'menu appended to body');
    assert.equal(menu.children.length, 2);

    // Clicking the second item fires onSelect('b') and closes the menu.
    menu.children[1].click();
    assert.deepEqual(received, ['b']);
});

test('PickleUI.select labels the trigger with the selected option', () => {
    const source = loadForm();
    const document = makeDom();
    const trigger = document.createElement('button');
    const sandbox = { window: {}, document };
    sandbox.window.document = document;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const handle = sandbox.window.PickleUI.select(trigger, {
        options: [{ value: 'a', label: 'Alpha' }, { value: 'b', label: 'Beta' }],
        onSelect: () => {},
    });

    handle.setValue('b');
    assert.equal(trigger.textContent, 'Beta');
});
```

- [ ] **Step 2: Run to verify fail**

Run: `node --test frontend/tests/ui/form.test.js`
Expected: FAIL (file not found).

### Task 3.3: Write `ui/form.js` with `PickleUI.select`

**Files:**
- Create: `frontend/static/app/ui/form.js`
- Modify: `frontend/index.html`

- [ ] **Step 1: Write the helper**

```javascript
// frontend/static/app/ui/form.js
/*
 * Form helpers.
 *
 * Hosts PickleUI.select — a custom select popover that renders a
 * .dropdown-menu-shaped list below the trigger. PR #5 refactors this
 * to delegate to PickleUI.dropdown; until then, the popover is inlined
 * here so form.js lands without depending on an unshipped primitive.
 */
(function initForm(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});

    function select(trigger, opts) {
        if (!trigger) {
            throw new Error('PickleUI.select: trigger element required');
        }
        const options = Array.isArray(opts && opts.options) ? opts.options : [];
        const onSelect = (opts && opts.onSelect) || (() => {});
        const placement = (opts && opts.placement) || 'bottom';

        let menu = null;
        let current = null;
        let outsideHandler = null;
        let escHandler = null;

        trigger.classList.add('select-trigger');
        trigger.setAttribute('aria-haspopup', 'listbox');
        trigger.setAttribute('aria-expanded', 'false');
        if (!trigger.textContent) {
            trigger.textContent = options[0] ? options[0].label : '';
        }

        function close() {
            if (!menu) return;
            menu.remove();
            menu = null;
            trigger.setAttribute('aria-expanded', 'false');
            if (outsideHandler) {
                global.document.removeEventListener('mousedown', outsideHandler, true);
                outsideHandler = null;
            }
            if (escHandler) {
                global.document.removeEventListener('keydown', escHandler, true);
                escHandler = null;
            }
        }

        function open() {
            if (menu) return;
            const doc = global.document;
            menu = doc.createElement('div');
            menu.classList.add('dropdown-menu');
            menu.setAttribute('role', 'listbox');

            for (const opt of options) {
                const item = doc.createElement('button');
                item.setAttribute('type', 'button');
                item.setAttribute('role', 'option');
                item.classList.add('dropdown-item');
                if (opt.value === current) item.classList.add('is-active');
                item.dataset.value = String(opt.value);
                item.textContent = opt.label;
                item.addEventListener('click', (event) => {
                    event.stopPropagation();
                    setValue(opt.value);
                    onSelect(opt.value);
                    close();
                });
                menu.appendChild(item);
            }

            // Position: below-left of trigger (top placement mirrors the offset).
            const rect = trigger.getBoundingClientRect();
            menu.style.position = 'fixed';
            menu.style.left = rect.left + 'px';
            menu.style.minWidth = Math.max(rect.width, 160) + 'px';
            menu.style.zIndex = 'var(--z-dropdown)';
            if (placement === 'top') {
                menu.style.bottom = (global.innerHeight - rect.top + 4) + 'px';
            } else {
                menu.style.top = (rect.bottom + 4) + 'px';
            }

            doc.body.appendChild(menu);
            trigger.setAttribute('aria-expanded', 'true');

            outsideHandler = (event) => {
                if (!trigger.contains(event.target) && !(menu && menu.contains(event.target))) {
                    close();
                }
            };
            escHandler = (event) => {
                if (event.key === 'Escape') close();
            };
            doc.addEventListener('mousedown', outsideHandler, true);
            doc.addEventListener('keydown', escHandler, true);
        }

        function setValue(value) {
            current = value;
            const match = options.find((o) => o.value === value);
            if (match) trigger.textContent = match.label;
        }

        trigger.addEventListener('click', (event) => {
            event.preventDefault();
            if (menu) close(); else open();
        });

        return { open, close, setValue, getValue: () => current };
    }

    PickleUI.select = select;
})(typeof window !== 'undefined' ? window : globalThis);
```

- [ ] **Step 2: Wire it into `index.html`**

Add the script tag immediately after `ui/00-namespace.js`:

```html
<script src="static/app/ui/00-namespace.js"></script>
<script src="static/app/ui/form.js"></script>
<script src="static/app/00-core.js"></script>
```

- [ ] **Step 3: Run the test**

Run: `node --test frontend/tests/ui/form.test.js`
Expected: PASS.

### Task 3.4: Migrate Settings form rows to `.input`, `.labeled-row`, `PickleUI.select`

**Files:**
- Modify: `frontend/index.html`
- Modify: `frontend/static/app/05-settings.js`

- [ ] **Step 1: Replace native `<select>` for verification provider**

```html
<!-- before: <select class="key-input" id="verify-provider-select">…</select> -->
<div class="field">
    <label class="field-label" for="verify-provider-select">Verification Provider</label>
    <button id="verify-provider-select" type="button" class="btn select-trigger"
            aria-haspopup="listbox" aria-expanded="false">Auto (prefer OpenAI)</button>
    <p class="field-hint">Choose which provider pickle should use for datasheet verification.</p>
</div>
```

- [ ] **Step 2: Wire `PickleUI.select` in `05-settings.js`**

Find the block that populates `#verify-provider-select` from native `<option>` nodes. Replace it with:

```javascript
const providerSelectEl = document.getElementById('verify-provider-select');
const providerHandle = window.PickleUI.select(providerSelectEl, {
    options: [
        { value: 'auto', label: 'Auto (prefer OpenAI)' },
        { value: 'openai', label: 'OpenAI only' },
        { value: 'anthropic', label: 'Anthropic only' },
    ],
    onSelect: (value) => { /* existing save logic, using `value` directly */ },
});
// initial value
providerHandle.setValue(currentProvider || 'auto');
```

Remove the previous `addEventListener('change', ...)` native-select code path.

- [ ] **Step 3: Replace key inputs with `.input`**

```html
<input type="password" class="input key-input" id="key-input-openai"
       placeholder="sk-proj-..." autocomplete="off" spellcheck="false">

<!-- (same for anthropic) -->
```

- [ ] **Step 4: Convert `.key-row` to `.labeled-row` where structure allows**

If the existing three-column layout in `04-shell-layout.css` for `.key-row` already matches `.labeled-row`, add `labeled-row` as an additional class; otherwise keep `.key-row` until CSS cleanup in the next step. Leave the data-attribute hooks untouched.

### Task 3.5: Migrate Package dialog input + part-picker input

**Files:**
- Modify: `frontend/index.html`

- [ ] **Step 1: Package dialog text input**

```html
<input id="package-name-input" type="text" class="input"
       placeholder="Enter the name to show in the UI" autocomplete="off">
```

- [ ] **Step 2: Part picker input**

```html
<input type="text" id="part-input" class="input"
       placeholder="Part number (e.g. DSPIC33CK64MP102)"
       autocomplete="off" spellcheck="false"
       aria-autocomplete="list" aria-controls="part-suggestions" aria-expanded="false">
```

- [ ] **Step 3: Oscillator / crystal numeric inputs**

```html
<input type="number" id="osc-crystal" class="input" value="8" min="0.032" max="64" step="0.001">
<input type="number" id="osc-target" class="input" value="200" min="1" max="200" step="0.001">
```

The native `<select>` pickers for `#osc-source` and `#osc-poscmd` remain native for now — Lane A does not mandate converting every `<select>` in one pass. File a follow-up TODO if the layout ends up inconsistent.

### Task 3.6: Delete legacy form CSS

**Files:**
- Modify: `frontend/static/styles/02-package-config.css`
- Modify: `frontend/static/styles/04-shell-layout.css`

- [ ] **Step 1: Remove now-dead rules**

Delete the following from `frontend/static/styles/04-shell-layout.css`:

- `.key-input` rule block (currently around line 1299) — all input styling now comes from `.input`
- `.key-reveal` button-specific rules — already covered by `.btn` in PR #2 but verify
- `.key-actions` button-specific properties only; keep the `display: flex` / `gap` layout rule

Delete the following **ID-selector** rules from the same file (note: they are `#package-name-input`, not `.package-name-input` — the PR #2 class-only sanity gate taught us that ID rules can override the primitive via specificity):

- `#package-name-input { ... }` base rule (currently around lines 998–1006)
- `#package-name-input:focus { ... }` (lines 1008–1011)
- `#package-name-input:disabled { ... }` (lines 1023–1026)

Keep `.package-dialog-input-wrap` — it holds the grid layout, not input styling.

In `frontend/static/styles/02-package-config.css`, scan for any `input { ... }` or `input[type="..."] { ... }` global resets that would conflict with `.input`. Scope them (e.g. restrict to `.pin-input`) or delete them outright.

If a generic `input { ... }` rule lives in `00-foundation.css`, trim it to just the resets `.input` does NOT own (e.g. `font: inherit`).

- [ ] **Step 2: Sanity gate (class + ID coverage)**

Expanded from PR #2's class-only gate to catch ID selectors too:

```bash
rg '^\s*(\.|#)(key-input|package-name-input|part-input)(\b|:|,|\s)' frontend/static/styles/
```

Expected: zero matches.

### Task 3.7: Commit PR #3

- [ ] **Step 1: Run full validate**

Run: `scripts/validate.sh`
Expected: PASS.

- [ ] **Step 2: Manual smoke test**

- Launch `cargo tauri dev`, open Settings, change verification provider via the new select, save/clear an API key — confirm behavior is unchanged.
- Type into the part picker, verify suggestions still behave (PR #5 will migrate them).

- [ ] **Step 3: Commit**

```bash
git add frontend/static/styles/components/form.css frontend/static/style.css frontend/static/app/ui/form.js frontend/tests/ui/form.test.js frontend/index.html frontend/static/app/05-settings.js frontend/static/styles/02-package-config.css frontend/static/styles/04-shell-layout.css
git commit -m "$(cat <<'EOF'
Lane A: form controls primitive

- Add components/form.css (input, select-trigger, stepper, checkbox, switch, labeled-row, field)
- Add ui/form.js with PickleUI.select (inline popover; PR #5 moves to PickleUI.dropdown)
- Migrate verification-provider to PickleUI.select, settings API-key inputs, package-dialog and part-picker inputs, oscillator numeric inputs
- Delete .key-input class rule and #package-name-input ID rules
EOF
)"
```

- [ ] **Step 4: Append logbook entry**

`Lane A PR #3: form primitive landed. Migrated verify-provider to PickleUI.select, swept inputs to .input.`

---

## PR #4 — Tooltip + Status bar + Toast

Three feedback atoms land together: tooltip (replaces the ad-hoc `[data-tip]` logic + native `title=`), status bar (semantic `PickleUI.status(text, tone)`), and toast (bottom-right stack). Status bar and toast coexist: status bar reflects current app state, toasts convey transient events.

### Task 4.1: Tooltip — tests

**Files:**
- Create: `frontend/tests/ui/tooltip.test.js`

- [ ] **Step 1: Write the failing test**

```javascript
// frontend/tests/ui/tooltip.test.js
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function loadTooltip() {
    const ns = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const tt = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'tooltip.js'), 'utf8');
    return ns + '\n' + tt;
}

function fakeDoc() {
    const listeners = {};
    const body = { _children: [], appendChild(el) { this._children.push(el); } };
    return {
        body,
        addEventListener(t, fn) { (listeners[t] ||= []).push(fn); },
        removeEventListener(t, fn) { const a = listeners[t] || []; const i = a.indexOf(fn); if (i !== -1) a.splice(i, 1); },
        dispatch(t, ev) { for (const fn of (listeners[t] || [])) fn(ev); },
        createElement(tag) {
            const cl = new Set();
            return {
                tagName: tag.toUpperCase(), style: {}, attributes: {}, children: [], textContent: '',
                classList: { add: (c) => cl.add(c), remove: (c) => cl.delete(c), contains: (c) => cl.has(c) },
                setAttribute(n, v) { this.attributes[n] = v; },
                appendChild(c) { this.children.push(c); },
                getBoundingClientRect() { return { left: 0, top: 0, right: 0, bottom: 0, width: 0, height: 0 }; },
                get offsetHeight() { return 20; },
                get offsetWidth() { return 100; },
            };
        },
    };
}

test('PickleUI.tooltip.install captures title= and strips it', () => {
    const source = loadTooltip();
    const document = fakeDoc();
    const window = { innerWidth: 800, innerHeight: 600, document };
    document.defaultView = window;
    const sandbox = { window, document };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const el = {
        attributes: { title: 'Hello' },
        dataset: {},
        getAttribute(n) { return this.attributes[n]; },
        removeAttribute(n) { delete this.attributes[n]; },
        setAttribute(n, v) { this.attributes[n] = v; },
    };

    sandbox.window.PickleUI.tooltip.capture(el);
    assert.equal(el.attributes.title, undefined);
    assert.equal(el.dataset.tip, 'Hello');
});

test('PickleUI.tooltip.capture prefers existing [data-tip] over [title]', () => {
    const source = loadTooltip();
    const document = fakeDoc();
    const window = { innerWidth: 800, innerHeight: 600, document };
    document.defaultView = window;
    const sandbox = { window, document };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const el = {
        attributes: { title: 'Native' },
        dataset: { tip: 'Custom' },
        getAttribute(n) { return this.attributes[n]; },
        removeAttribute(n) { delete this.attributes[n]; },
        setAttribute(n, v) { this.attributes[n] = v; },
    };

    sandbox.window.PickleUI.tooltip.capture(el);
    assert.equal(el.attributes.title, undefined);
    assert.equal(el.dataset.tip, 'Custom');
});

test('PickleUI.tooltip exposes show/hide helpers', () => {
    const source = loadTooltip();
    const document = fakeDoc();
    const window = { innerWidth: 800, innerHeight: 600, document };
    document.defaultView = window;
    const sandbox = { window, document };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    assert.equal(typeof sandbox.window.PickleUI.tooltip.show, 'function');
    assert.equal(typeof sandbox.window.PickleUI.tooltip.hide, 'function');
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --test frontend/tests/ui/tooltip.test.js`
Expected: FAIL.

### Task 4.2: Tooltip — CSS + helper

**Files:**
- Create: `frontend/static/styles/components/tooltip.css`
- Create: `frontend/static/app/ui/tooltip.js`
- Modify: `frontend/static/style.css`
- Modify: `frontend/index.html`

- [ ] **Step 1: Write `tooltip.css`**

```css
/*
 * Tooltip primitive.
 *
 * Inverted surface: text color = --text, background = --bg-primary
 * read from the *opposite* theme. In practice we set surface to
 * --text and text to --bg-primary so the popover contrasts with
 * the surrounding app regardless of theme.
 */
.tooltip {
    position: fixed;
    z-index: var(--z-tooltip);
    padding: var(--space-3) var(--space-5);
    border-radius: var(--radius-sm);
    background: var(--text);
    color: var(--bg);
    font-size: var(--text-sm);
    line-height: 1.4;
    max-width: 240px;
    box-shadow: var(--shadow-md);
    pointer-events: none;
    opacity: 0;
    transition: opacity var(--motion-fast) ease;
}

.tooltip.is-visible {
    opacity: 1;
}

.tooltip-arrow {
    position: absolute;
    width: 0;
    height: 0;
    border-left: 4px solid transparent;
    border-right: 4px solid transparent;
}

.tooltip.is-above .tooltip-arrow {
    bottom: -4px;
    left: 8px;
    border-top: 4px solid var(--text);
}

.tooltip.is-below .tooltip-arrow {
    top: -4px;
    left: 8px;
    border-bottom: 4px solid var(--text);
}
```

- [ ] **Step 2: Write `ui/tooltip.js`**

```javascript
// frontend/static/app/ui/tooltip.js
/*
 * Tooltip helper.
 *
 * Installs a single reusable tooltip element; attaches pointer/focus
 * listeners at the document level; captures [title] into [data-tip] on
 * first sighting so the native tooltip does not double-render.
 */
(function initTooltip(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});
    const DELAY_MS = 300;

    let element = null;
    let arrow = null;
    let timer = null;
    let installed = false;

    function ensureElement() {
        if (element) return;
        const doc = global.document;
        element = doc.createElement('div');
        element.classList.add('tooltip');
        arrow = doc.createElement('div');
        arrow.classList.add('tooltip-arrow');
        element.appendChild(arrow);
        doc.body.appendChild(element);
    }

    function capture(el) {
        if (!el) return;
        if (el.dataset && el.dataset.tip) {
            if (el.getAttribute && el.getAttribute('title')) {
                el.removeAttribute('title');
            }
            return;
        }
        const title = el.getAttribute && el.getAttribute('title');
        if (title) {
            el.dataset.tip = title;
            el.removeAttribute('title');
        }
    }

    function show(target) {
        if (!target || !target.dataset || !target.dataset.tip) return;
        ensureElement();
        const doc = global.document;
        element.textContent = target.dataset.tip;
        // Re-append arrow because textContent reset cleared children.
        element.appendChild(arrow);
        element.classList.add('is-visible');

        const rect = target.getBoundingClientRect();
        const tipH = element.offsetHeight;
        const tipW = element.offsetWidth;
        let top = rect.top - tipH - 6;
        let above = true;
        if (top < 4) {
            top = rect.bottom + 6;
            above = false;
        }
        let left = rect.left;
        const maxLeft = global.innerWidth - tipW - 4;
        if (left > maxLeft) left = maxLeft;
        if (left < 4) left = 4;

        element.classList.toggle('is-above', above);
        element.classList.toggle('is-below', !above);
        element.style.top = top + 'px';
        element.style.left = left + 'px';
    }

    function hide() {
        if (!element) return;
        element.classList.remove('is-visible');
    }

    function onOver(event) {
        const el = event.target && event.target.closest && event.target.closest('[data-tip], [title]');
        if (!el) return;
        capture(el);
        if (timer) clearTimeout(timer);
        timer = setTimeout(() => show(el), DELAY_MS);
    }

    function onOut(event) {
        const el = event.target && event.target.closest && event.target.closest('[data-tip]');
        if (!el) return;
        if (timer) { clearTimeout(timer); timer = null; }
        hide();
    }

    function install() {
        if (installed) return;
        installed = true;
        const doc = global.document;
        // Initial sweep: capture every [title] currently in the DOM so the
        // native tooltip never appears.
        if (doc.querySelectorAll) {
            for (const el of doc.querySelectorAll('[title]')) capture(el);
        }
        doc.addEventListener('mouseover', onOver);
        doc.addEventListener('mouseout', onOut);
        doc.addEventListener('focusin', onOver);
        doc.addEventListener('focusout', onOut);
    }

    PickleUI.tooltip = { install, capture, show, hide };
})(typeof window !== 'undefined' ? window : globalThis);
```

- [ ] **Step 3: Wire into style.css and index.html**

Add to `style.css`:
```css
@import url("styles/components/tooltip.css");
```

Add to `index.html` right after `ui/form.js`:
```html
<script src="static/app/ui/tooltip.js"></script>
```

- [ ] **Step 4: Run the test**

Run: `node --test frontend/tests/ui/tooltip.test.js`
Expected: PASS.

### Task 4.3: Remove ad-hoc tooltip code from `08-bootstrap.js`

**Files:**
- Modify: `frontend/static/app/08-bootstrap.js`
- Modify: `frontend/static/styles/04-shell-layout.css` (where `.app-tooltip` currently lives, around lines 735+)

- [ ] **Step 1: Replace `wireTooltipSystem()` call and body**

In `08-bootstrap.js`:
```javascript
// replace the let/function pair and call with a single install() line
PickleUI.tooltip.install();
```
Delete the `let tooltipEventsBound` flag, the `wireTooltipSystem()` function, and the now-dead `.app-tooltip` CSS block.

- [ ] **Step 2: Search for `.app-tooltip` CSS**

```bash
rg '\.app-tooltip' frontend/static/
```
Remove any matching rules; the class is unused.

- [ ] **Step 3: Validate**

Run: `scripts/validate.sh`
Expected: PASS.

### Task 4.4: Commit tooltip slice

- [ ] **Step 1: Commit**

```bash
git add frontend/static/styles/components/tooltip.css frontend/static/app/ui/tooltip.js frontend/tests/ui/tooltip.test.js frontend/static/style.css frontend/index.html frontend/static/app/08-bootstrap.js frontend/static/styles/04-shell-layout.css
git commit -m "$(cat <<'EOF'
Lane A: tooltip primitive

- Add components/tooltip.css (inverted surface so popover contrasts with theme)
- Add ui/tooltip.js with PickleUI.tooltip.{install,capture,show,hide}
- Capture [title] into [data-tip] on sighting; install at bootstrap
- Remove ad-hoc wireTooltipSystem() from 08-bootstrap.js and .app-tooltip CSS
EOF
)"
```

### Task 4.5: Status bar — test + CSS + helper

**Files:**
- Create: `frontend/tests/ui/status-bar.test.js`
- Create: `frontend/static/styles/components/status-bar.css`
- Create: `frontend/static/app/ui/status-bar.js`
- Modify: `frontend/static/style.css`
- Modify: `frontend/index.html`

- [ ] **Step 1: Write the failing test**

```javascript
// frontend/tests/ui/status-bar.test.js
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function loadStatus() {
    const ns = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const sb = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'status-bar.js'), 'utf8');
    return ns + '\n' + sb;
}

function makeStatusEl() {
    const cl = new Set();
    return {
        textContent: '',
        attributes: {},
        classList: {
            add: (c) => cl.add(c),
            remove: (c) => cl.delete(c),
            contains: (c) => cl.has(c),
            _set: cl,
        },
        setAttribute(n, v) { this.attributes[n] = v; },
        getAttribute(n) { return this.attributes[n]; },
    };
}

test('PickleUI.status sets text and tone class', () => {
    const source = loadStatus();
    const statusEl = makeStatusEl();
    const document = { getElementById: (id) => (id === 'status' ? statusEl : null) };
    const sandbox = { window: {}, document };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.status('Loading...', 'busy');
    assert.equal(statusEl.textContent, 'Loading...');
    assert.equal(statusEl.classList.contains('status-bar-tone-busy'), true);

    sandbox.window.PickleUI.status('Done', 'success');
    assert.equal(statusEl.textContent, 'Done');
    assert.equal(statusEl.classList.contains('status-bar-tone-busy'), false);
    assert.equal(statusEl.classList.contains('status-bar-tone-success'), true);
});

test('PickleUI.status defaults to idle tone', () => {
    const source = loadStatus();
    const statusEl = makeStatusEl();
    const document = { getElementById: (id) => (id === 'status' ? statusEl : null) };
    const sandbox = { window: {}, document };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.status('Ready');
    assert.equal(statusEl.classList.contains('status-bar-tone-idle'), true);
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --test frontend/tests/ui/status-bar.test.js`
Expected: FAIL.

- [ ] **Step 3: Write `status-bar.css`**

```css
/*
 * Status bar primitive.
 *
 * The footer element (#status inside <footer.status-bar>) is the only
 * persistent app-state line. Tones: idle / busy / success / warn / error.
 * busy adds a rotating spinner via ::before.
 */
.status-bar {
    padding: var(--space-3) var(--space-7);
    border-top: 1px solid var(--border);
    background: var(--bg-card);
    font-size: var(--text-sm);
    color: var(--text-dim);
    display: flex;
    align-items: center;
    gap: var(--space-3);
}

.status-bar-tone-idle { color: var(--text-dim); }
.status-bar-tone-success { color: var(--status-good); }
.status-bar-tone-warn { color: var(--status-warn); }
.status-bar-tone-error { color: var(--error); }

.status-bar-tone-busy {
    color: var(--accent);
}

.status-bar-tone-busy::before {
    content: "";
    display: inline-block;
    width: 12px;
    height: 12px;
    margin-right: var(--space-3);
    border: 2px solid currentColor;
    border-top-color: transparent;
    border-radius: var(--radius-full);
    animation: status-bar-spin 0.8s linear infinite;
    flex-shrink: 0;
}

@keyframes status-bar-spin {
    to { transform: rotate(360deg); }
}
```

- [ ] **Step 4: Write `ui/status-bar.js`**

```javascript
// frontend/static/app/ui/status-bar.js
/*
 * Status bar helper.
 *
 * Single API: PickleUI.status(text, tone).
 * tone ∈ 'idle' | 'busy' | 'success' | 'warn' | 'error'. Previous tone
 * class is cleared before the new one is applied.
 */
(function initStatus(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});
    const TONES = ['idle', 'busy', 'success', 'warn', 'error'];

    function setStatus(text, tone) {
        const el = global.document.getElementById('status');
        if (!el) return;
        const normalized = TONES.includes(tone) ? tone : 'idle';
        el.textContent = String(text == null ? '' : text);
        for (const t of TONES) {
            el.classList.remove('status-bar-tone-' + t);
        }
        el.classList.add('status-bar-tone-' + normalized);
    }

    PickleUI.status = setStatus;
})(typeof window !== 'undefined' ? window : globalThis);
```

- [ ] **Step 5: Wire imports**

Add to `style.css`:
```css
@import url("styles/components/status-bar.css");
```

Add to `index.html`:
```html
<script src="static/app/ui/status-bar.js"></script>
```

- [ ] **Step 6: Run test**

Run: `node --test frontend/tests/ui/status-bar.test.js`
Expected: PASS.

### Task 4.6: Refactor every `setStatus()` call site to pass explicit tone

**Files:**
- Modify: `frontend/static/app/00-core.js`
- Modify: `frontend/static/app/06-shell.js`
- Modify: `frontend/static/app/07-verification-render.js`
- Modify: `frontend/static/app/07-verification.js`
- Modify: `frontend/static/app/08-bootstrap.js`
- Plus any other `setStatus` callers (see ripgrep)

- [ ] **Step 1: Enumerate call sites**

```bash
rg -n '\bsetStatus\(' frontend/static/app/
```

- [ ] **Step 2: Replace body of `setStatus` in `00-core.js`**

Replace `function setStatus(msg) { ... keyword-sniffing ... }` with:

```javascript
/** Update the bottom status bar with an explicit tone. */
function setStatus(msg, tone) {
    window.PickleUI.status(msg == null || msg === '' ? 'Ready' : msg, tone || 'idle');
}
```

- [ ] **Step 3: Update each caller to pass tone**

Enumerate each caller and pick the tone from the message's semantic meaning (not keyword matching). Typical rewrites:

```javascript
// 00-core.js
setStatus(appConfig.ui.deviceLoad.cachedStatus);                       // before
setStatus(appConfig.ui.deviceLoad.cachedStatus, 'success');            // after
setStatus(`${deviceData.part_number} — …`);                            // before
setStatus(`${deviceData.part_number} — …`, 'success');                 // after
setStatus('Error: ' + (e.message || e));                               // before
setStatus('Error: ' + (e.message || e), 'error');                      // after
setStatus(`Error saving package name: …`);                             // before
setStatus(`Error saving package name: …`, 'error');                    // after
setStatus(appConfig.format(appConfig.ui.packageManager.resetStatus, …)); // before
setStatus(appConfig.format(appConfig.ui.packageManager.resetStatus, …), 'success');

// 06-shell.js — busy during long operations
setStatus('Loading device…');                                          // before
setStatus('Loading device…', 'busy');                                  // after

// 07-verification*.js
setStatus('Verifying pinout…');                                        // before
setStatus('Verifying pinout…', 'busy');                                // after
setStatus('Verification complete');                                    // before
setStatus('Verification complete', 'success');                         // after
```

Every caller that previously depended on keyword-sniffing must explicitly choose `idle`, `busy`, `success`, `warn`, or `error`. When in doubt, `idle` is the safe default.

- [ ] **Step 4: Validate**

Run: `scripts/validate.sh`
Expected: PASS.

- [ ] **Step 5: Launch and smoke test**

Run: `cargo tauri dev`. Trigger an intentional error path (e.g. load a non-existent part number) — confirm the status bar turns red with the error tone. Trigger a success path (load a known part) — confirm the text turns green.

- [ ] **Step 6: Commit status-bar slice**

```bash
git add frontend/static/styles/components/status-bar.css frontend/static/app/ui/status-bar.js frontend/tests/ui/status-bar.test.js frontend/static/style.css frontend/index.html frontend/static/app/00-core.js frontend/static/app/06-shell.js frontend/static/app/07-verification-render.js frontend/static/app/07-verification.js frontend/static/app/08-bootstrap.js
git commit -m "$(cat <<'EOF'
Lane A: status bar primitive

- Add components/status-bar.css with explicit tone classes + spinner for busy
- Add ui/status-bar.js exporting PickleUI.status(text, tone)
- Replace keyword-sniffing in 00-core.js setStatus with a thin shim that forwards to PickleUI.status
- Every setStatus() call site now passes an explicit tone
EOF
)"
```

### Task 4.7: Toast — tests

**Files:**
- Create: `frontend/tests/ui/toast.test.js`

- [ ] **Step 1: Write the failing test**

```javascript
// frontend/tests/ui/toast.test.js
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function load() {
    const ns = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const t = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'toast.js'), 'utf8');
    return ns + '\n' + t;
}

function mkDoc() {
    const body = {
        _children: [],
        appendChild(el) { this._children.push(el); el.parentNode = this; },
    };
    return {
        body,
        _timers: [],
        createElement(tag) {
            const cl = new Set();
            const el = {
                tagName: tag.toUpperCase(), children: [], textContent: '', attributes: {},
                classList: { add: (c) => cl.add(c), remove: (c) => cl.delete(c), contains: (c) => cl.has(c) },
                appendChild(c) { this.children.push(c); c.parentNode = this; },
                setAttribute(n, v) { this.attributes[n] = v; },
                addEventListener() {},
                remove() {
                    if (this.parentNode) {
                        const i = this.parentNode._children ? this.parentNode._children.indexOf(this) : this.parentNode.children.indexOf(this);
                        const arr = this.parentNode._children || this.parentNode.children;
                        if (i !== -1) arr.splice(i, 1);
                    }
                },
            };
            return el;
        },
    };
}

test('PickleUI.toast appends to stack and returns a handle', () => {
    const doc = mkDoc();
    const sandbox = { window: {}, document: doc, setTimeout: (fn, ms) => { doc._timers.push({ fn, ms }); return doc._timers.length; }, clearTimeout: () => {} };
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    const handle = sandbox.window.PickleUI.toast('Hello');
    const stack = doc.body._children[0];
    assert.equal(stack.children.length, 1);
    assert.equal(typeof handle.dismiss, 'function');
    assert.equal(typeof handle.update, 'function');
});

test('PickleUI.toast error tone is sticky', () => {
    const doc = mkDoc();
    const sandbox = { window: {}, document: doc, setTimeout: (fn, ms) => { doc._timers.push({ fn, ms }); return doc._timers.length; }, clearTimeout: () => {} };
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    sandbox.window.PickleUI.toast('Broke', { tone: 'error' });
    assert.equal(doc._timers.length, 0, 'error toast must not schedule auto-dismiss');
});

test('PickleUI.toast info tone auto-dismisses after 5 s', () => {
    const doc = mkDoc();
    const sandbox = { window: {}, document: doc, setTimeout: (fn, ms) => { doc._timers.push({ fn, ms }); return doc._timers.length; }, clearTimeout: () => {} };
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    sandbox.window.PickleUI.toast('Hello', { tone: 'info' });
    assert.equal(doc._timers.length, 1);
    assert.equal(doc._timers[0].ms, 5000);
});

test('PickleUI.toast stack caps visible toasts at 5 and evicts oldest auto-dismiss first', () => {
    const doc = mkDoc();
    const sandbox = { window: {}, document: doc, setTimeout: () => 1, clearTimeout: () => {} };
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    for (let i = 0; i < 6; i += 1) {
        sandbox.window.PickleUI.toast('t' + i, { tone: 'info' });
    }
    const stack = doc.body._children[0];
    assert.equal(stack.children.length, 5);
});

test('PickleUI.toast.update mutates title/body/progress', () => {
    const doc = mkDoc();
    const sandbox = { window: {}, document: doc, setTimeout: () => 1, clearTimeout: () => {} };
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    const h = sandbox.window.PickleUI.toast('Working', { tone: 'progress', title: 'Verify' });
    h.update({ title: 'Verify done', body: 'All good', progress: 1 });
    // find the toast node in the stack
    const stack = doc.body._children[0];
    const toastEl = stack.children[0];
    // title + body children
    const titleEl = toastEl.children.find((c) => c.classList.contains('toast-title'));
    const bodyEl = toastEl.children.find((c) => c.classList.contains('toast-body'));
    assert.equal(titleEl.textContent, 'Verify done');
    assert.equal(bodyEl.textContent, 'All good');
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --test frontend/tests/ui/toast.test.js`
Expected: FAIL.

### Task 4.8: Toast — CSS + helper

**Files:**
- Create: `frontend/static/styles/components/toast.css`
- Create: `frontend/static/app/ui/toast.js`
- Modify: `frontend/static/style.css`
- Modify: `frontend/index.html`

- [ ] **Step 1: Write `toast.css`**

```css
/*
 * Toast primitive.
 *
 * Bottom-right stack of transient notifications. Tone-colored 3 px left
 * stripe. Stack limit 5; excess auto-dismissable toasts are evicted FIFO.
 * error + progress tones are sticky (no auto-dismiss).
 */
.toast-stack {
    position: fixed;
    bottom: var(--space-6);
    right: var(--space-6);
    display: flex;
    flex-direction: column-reverse;
    gap: var(--space-5);
    z-index: var(--z-toast);
    pointer-events: none;
    max-width: 420px;
}

.toast {
    position: relative;
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: var(--space-4);
    align-items: start;
    padding: var(--space-5) var(--space-6);
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-left-width: 3px;
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
    min-width: 300px;
    max-width: 400px;
    pointer-events: auto;
    animation: toast-slide-in var(--motion-medium) ease;
}

@keyframes toast-slide-in {
    from { opacity: 0; transform: translateY(6px); }
    to   { opacity: 1; transform: translateY(0); }
}

.toast-info     { border-left-color: var(--text-dim); }
.toast-success  { border-left-color: var(--status-good); }
.toast-warn     { border-left-color: var(--status-warn); }
.toast-error    { border-left-color: var(--error); }
.toast-progress { border-left-color: var(--accent); }

.toast-icon {
    font-size: var(--text-xl);
    line-height: 1;
    color: var(--text-dim);
    align-self: start;
}

.toast-info     .toast-icon { color: var(--text-dim); }
.toast-success  .toast-icon { color: var(--status-good); }
.toast-warn     .toast-icon { color: var(--status-warn); }
.toast-error    .toast-icon { color: var(--error); }
.toast-progress .toast-icon { color: var(--accent); }

.toast-title {
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    color: var(--text);
    line-height: 1.3;
}

.toast-body {
    font-size: var(--text-sm);
    color: var(--text-dim);
    margin-top: var(--space-1);
    line-height: var(--leading-normal);
}

.toast-actions {
    display: flex;
    gap: var(--space-3);
    align-items: center;
}

.toast-dismiss {
    width: 18px;
    height: 18px;
    border: 0;
    padding: 0;
    background: transparent;
    color: var(--text-dim);
    font-size: var(--text-lg);
    cursor: pointer;
}

.toast-dismiss:hover {
    color: var(--text);
}

.toast-progress-bar {
    grid-column: 1 / -1;
    height: 3px;
    border-radius: var(--radius-full);
    background: var(--hover-overlay);
    margin-top: var(--space-3);
    overflow: hidden;
}

.toast-progress-bar-inner {
    display: block;
    height: 100%;
    background: var(--accent);
    width: 0%;
    transition: width var(--motion-medium) ease;
}
```

- [ ] **Step 2: Write `ui/toast.js`**

```javascript
// frontend/static/app/ui/toast.js
/*
 * Toast helper.
 *
 * Stack limit of 5 visible toasts; oldest auto-dismiss toast (info /
 * success / warn) is evicted when a 6th is pushed. error + progress
 * never auto-dismiss and never auto-evict.
 */
(function initToast(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});
    const STACK_LIMIT = 5;
    const DEFAULT_DURATION = 5000;
    const ICONS = {
        info: 'i',
        success: '\u2713',
        warn: '!',
        error: '\u2716',
        progress: '\u21bb',
    };
    const TITLES = {
        info: 'Info',
        success: 'Success',
        warn: 'Warning',
        error: 'Error',
        progress: 'Working',
    };

    let stack = null;
    const handles = [];

    function ensureStack() {
        if (stack) return stack;
        const doc = global.document;
        stack = doc.createElement('div');
        stack.classList.add('toast-stack');
        doc.body.appendChild(stack);
        return stack;
    }

    function evictIfNeeded() {
        if (handles.length <= STACK_LIMIT) return;
        const idx = handles.findIndex((h) => h.autoDismiss);
        if (idx !== -1) handles[idx].dismiss();
    }

    function toast(message, opts) {
        const options = opts || {};
        const tone = ['info', 'success', 'warn', 'error', 'progress'].includes(options.tone)
            ? options.tone
            : 'info';
        const sticky = !!options.sticky || tone === 'error' || tone === 'progress';
        const duration = typeof options.duration === 'number' ? options.duration : DEFAULT_DURATION;

        const doc = global.document;
        const root = ensureStack();

        const el = doc.createElement('div');
        el.classList.add('toast');
        el.classList.add('toast-' + tone);
        el.setAttribute('role', tone === 'error' ? 'alert' : 'status');

        const iconEl = doc.createElement('span');
        iconEl.classList.add('toast-icon');
        iconEl.textContent = ICONS[tone];
        el.appendChild(iconEl);

        const content = doc.createElement('div');
        const titleEl = doc.createElement('div');
        titleEl.classList.add('toast-title');
        titleEl.textContent = options.title || TITLES[tone];
        content.appendChild(titleEl);
        const bodyEl = doc.createElement('div');
        bodyEl.classList.add('toast-body');
        bodyEl.textContent = String(message == null ? '' : message);
        content.appendChild(bodyEl);
        el.appendChild(content);

        const actions = doc.createElement('div');
        actions.classList.add('toast-actions');
        if (options.action && options.action.label) {
            const btn = doc.createElement('button');
            btn.setAttribute('type', 'button');
            btn.classList.add('btn');
            btn.classList.add('btn-sm');
            btn.classList.add('btn-ghost');
            btn.textContent = options.action.label;
            btn.addEventListener('click', () => {
                try { options.action.onClick && options.action.onClick(); } finally { handle.dismiss(); }
            });
            actions.appendChild(btn);
        }
        const dismissBtn = doc.createElement('button');
        dismissBtn.setAttribute('type', 'button');
        dismissBtn.setAttribute('aria-label', 'Dismiss');
        dismissBtn.classList.add('toast-dismiss');
        dismissBtn.textContent = '\u00d7';
        dismissBtn.addEventListener('click', () => handle.dismiss());
        actions.appendChild(dismissBtn);
        el.appendChild(actions);

        let progressInner = null;
        if (tone === 'progress') {
            const bar = doc.createElement('div');
            bar.classList.add('toast-progress-bar');
            progressInner = doc.createElement('span');
            progressInner.classList.add('toast-progress-bar-inner');
            bar.appendChild(progressInner);
            el.appendChild(bar);
        }

        root.appendChild(el);

        let timer = null;
        const autoDismiss = !sticky;
        const handle = {
            autoDismiss,
            update(next) {
                if (!next) return;
                if (typeof next.title === 'string') titleEl.textContent = next.title;
                if (typeof next.body === 'string' || typeof next.message === 'string') {
                    bodyEl.textContent = next.body != null ? next.body : next.message;
                }
                if (typeof next.progress === 'number' && progressInner) {
                    const pct = Math.max(0, Math.min(1, next.progress));
                    progressInner.style.width = (pct * 100) + '%';
                }
            },
            dismiss() {
                if (timer) { clearTimeout(timer); timer = null; }
                el.remove();
                const i = handles.indexOf(handle);
                if (i !== -1) handles.splice(i, 1);
            },
        };

        handles.push(handle);
        if (autoDismiss) {
            timer = setTimeout(handle.dismiss, duration);
        }
        evictIfNeeded();
        return handle;
    }

    PickleUI.toast = toast;
})(typeof window !== 'undefined' ? window : globalThis);
```

- [ ] **Step 3: Wire imports**

`style.css`:
```css
@import url("styles/components/toast.css");
```

`index.html`:
```html
<script src="static/app/ui/toast.js"></script>
```

- [ ] **Step 4: Run tests**

Run: `node --test frontend/tests/ui/toast.test.js`
Expected: PASS.

### Task 4.9: Wire existing transient signals through `PickleUI.toast`

**Files:**
- Modify: `frontend/static/app/05-config-files.js`
- Modify: `frontend/static/app/05-compile-check.js`

Scope of this task is deliberately narrow: only the signals that are **transient events with no persistent state to communicate** move to toast. Everything else stays on the status bar.

- [ ] **Step 1: Config save — success toast**

In `05-config-files.js`, after the existing `setStatus('Saved …', 'success')` line in the save flow, also emit:

```javascript
window.PickleUI.toast(path || 'Config', {
    tone: 'success',
    title: 'Saved',
    duration: 3000,
});
```

Leave the status bar message so the footer still reflects "saved at HH:MM" until the next action.

- [ ] **Step 2: Compile error → toast**

In `05-compile-check.js`, when the compile-check returns an error, replace the plain inline status message with:

```javascript
window.PickleUI.toast(errorMessage, {
    tone: 'error',
    title: 'Compile error',
    action: { label: 'Show', onClick: () => scrollToCompileResult() },
});
```

Keep the DOM-level error list rendering as-is.

- [ ] **Step 3: Validate + smoke**

Run: `scripts/validate.sh`
Expected: PASS.

Launch the app, save a config, verify a toast appears. Trigger a deliberate compile error, verify the error toast stays and the Show action scrolls to the compile result.

### Task 4.10: Commit toast slice

- [ ] **Step 1: Commit**

```bash
git add frontend/static/styles/components/toast.css frontend/static/app/ui/toast.js frontend/tests/ui/toast.test.js frontend/static/style.css frontend/index.html frontend/static/app/05-config-files.js frontend/static/app/05-compile-check.js
git commit -m "$(cat <<'EOF'
Lane A: toast primitive

- Add components/toast.css (stack, tones, progress bar, dismiss)
- Add ui/toast.js with PickleUI.toast(msg, opts); stack limit 5, error + progress sticky
- Wire config-save success and compile-check error as toasts, status bar keeps persistent state
EOF
)"
```

- [ ] **Step 2: Append logbook entry**

`Lane A PR #4: feedback-atoms primitive trio landed (tooltip, status bar, toast). Captured [title] + [data-tip], replaced keyword-sniffing with PickleUI.status(text, tone), added PickleUI.toast with stack limit 5.`

---

## PR #5 — Dropdown menu primitive

Adds `components/dropdown-menu.css` + `ui/dropdown.js`. Refactors `ui/form.js`'s inline popover to delegate to `PickleUI.dropdown`. Migrates the three feature-level menus: package actions, save menu, and part-picker suggestion list.

### Task 5.1: Dropdown — tests

**Files:**
- Create: `frontend/tests/ui/dropdown.test.js`

- [ ] **Step 1: Write the failing test**

```javascript
// frontend/tests/ui/dropdown.test.js
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function load() {
    const ns = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const d = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'dropdown.js'), 'utf8');
    return ns + '\n' + d;
}

function mkDoc() {
    const docListeners = {};
    const body = { _children: [], appendChild(el) { this._children.push(el); el.parentNode = this; }, contains(e) { return this._children.includes(e); } };
    function element(tag) {
        const cl = new Set();
        const l = {};
        const el = {
            tagName: tag.toUpperCase(), children: [], textContent: '', attributes: {}, dataset: {}, style: {}, parentNode: null,
            classList: { add: (c) => cl.add(c), remove: (c) => cl.delete(c), contains: (c) => cl.has(c) },
            appendChild(c) { this.children.push(c); c.parentNode = this; },
            setAttribute(n, v) { this.attributes[n] = v; },
            getAttribute(n) { return this.attributes[n]; },
            addEventListener(t, fn) { (l[t] ||= []).push(fn); },
            click() { for (const fn of (l.click || [])) fn({ target: el, stopPropagation() {}, preventDefault() {} }); },
            getBoundingClientRect() { return { left: 0, top: 0, right: 120, bottom: 28, width: 120, height: 28 }; },
            contains(x) { return x === el || this.children.some((c) => c.contains && c.contains(x)); },
            remove() { if (this.parentNode) { const arr = this.parentNode._children || this.parentNode.children; const i = arr.indexOf(this); if (i !== -1) arr.splice(i, 1); } this.parentNode = null; },
        };
        return el;
    }
    return {
        body,
        createElement: element,
        addEventListener(t, fn) { (docListeners[t] ||= []).push(fn); },
        removeEventListener(t, fn) { const a = docListeners[t] || []; const i = a.indexOf(fn); if (i !== -1) a.splice(i, 1); },
        dispatch(t, ev) { for (const fn of (docListeners[t] || [])) fn(ev); },
    };
}

test('PickleUI.dropdown opens on trigger click, fires onSelect, closes', () => {
    const source = load();
    const doc = mkDoc();
    const trigger = doc.createElement('button');
    const sandbox = { window: {}, document: doc };
    sandbox.window.document = doc;
    sandbox.window.innerHeight = 600;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const received = [];
    sandbox.window.PickleUI.dropdown(trigger, {
        items: [{ id: 'a', label: 'Alpha' }, { id: 'b', label: 'Beta' }],
        onSelect: (id) => received.push(id),
    });

    trigger.click();
    const menu = doc.body._children[doc.body._children.length - 1];
    assert.ok(menu);
    assert.equal(menu.children.length, 2);

    menu.children[1].click();
    assert.deepEqual(received, ['b']);
    assert.equal(doc.body._children.includes(menu), false, 'menu removed after select');
});

test('PickleUI.dropdown closes on Esc', () => {
    const source = load();
    const doc = mkDoc();
    const trigger = doc.createElement('button');
    const sandbox = { window: {}, document: doc };
    sandbox.window.document = doc;
    sandbox.window.innerHeight = 600;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.dropdown(trigger, {
        items: [{ id: 'a', label: 'Alpha' }],
        onSelect: () => {},
    });
    trigger.click();
    assert.ok(doc.body._children.length > 0);
    doc.dispatch('keydown', { key: 'Escape' });
    // Menu is removed; empty or same body with menu no longer present.
    assert.equal(doc.body._children.length === 0 || !doc.body._children[doc.body._children.length - 1].classList.contains('dropdown-menu'), true);
});

test('PickleUI.dropdown renders dividers and danger styling', () => {
    const source = load();
    const doc = mkDoc();
    const trigger = doc.createElement('button');
    const sandbox = { window: {}, document: doc };
    sandbox.window.document = doc;
    sandbox.window.innerHeight = 600;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.dropdown(trigger, {
        items: [
            { id: 'a', label: 'Alpha' },
            { divider: true },
            { id: 'd', label: 'Delete', danger: true },
        ],
        onSelect: () => {},
    });
    trigger.click();
    const menu = doc.body._children[doc.body._children.length - 1];
    assert.equal(menu.children.length, 3);
    assert.equal(menu.children[1].classList.contains('dropdown-divider'), true);
    assert.equal(menu.children[2].classList.contains('is-danger'), true);
});

test('PickleUI.dropdown accepts items as a factory and rebuilds on open', () => {
    const source = load();
    const doc = mkDoc();
    const trigger = doc.createElement('button');
    const sandbox = { window: {}, document: doc };
    sandbox.window.document = doc;
    sandbox.window.innerHeight = 600;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    let includeDelete = false;
    sandbox.window.PickleUI.dropdown(trigger, {
        items: () => {
            const list = [{ id: 'a', label: 'Alpha' }];
            if (includeDelete) list.push({ id: 'd', label: 'Delete', danger: true });
            return list;
        },
        onSelect: () => {},
    });

    trigger.click();
    let menu = doc.body._children[doc.body._children.length - 1];
    assert.equal(menu.children.length, 1, 'factory returns 1 item before state change');
    trigger.click(); // closes

    includeDelete = true;
    trigger.click();
    menu = doc.body._children[doc.body._children.length - 1];
    assert.equal(menu.children.length, 2, 'factory returns 2 items after state change');
});

test('PickleUI.dropdown renders optional item.meta as secondary text', () => {
    const source = load();
    const doc = mkDoc();
    const trigger = doc.createElement('button');
    const sandbox = { window: {}, document: doc };
    sandbox.window.document = doc;
    sandbox.window.innerHeight = 600;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.dropdown(trigger, {
        items: [{ id: 'a', label: 'Alpha', meta: 'Cached' }],
        onSelect: () => {},
    });
    trigger.click();
    const menu = doc.body._children[doc.body._children.length - 1];
    const metaSpan = menu.children[0].children.find((c) => c.classList.contains('dropdown-item-meta'));
    assert.ok(metaSpan, 'meta span rendered');
    assert.equal(metaSpan.textContent, 'Cached');
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --test frontend/tests/ui/dropdown.test.js`
Expected: FAIL.

### Task 5.2: Dropdown — CSS + helper

**Files:**
- Create: `frontend/static/styles/components/dropdown-menu.css`
- Create: `frontend/static/app/ui/dropdown.js`
- Modify: `frontend/static/style.css`
- Modify: `frontend/index.html`

- [ ] **Step 1: Write `dropdown-menu.css`**

```css
/*
 * Dropdown menu primitive.
 *
 * Single style covers the package actions menu, save menu, part picker
 * suggestions, and select popover. Items are 24 px tall; optional icon
 * column; optional divider; destructive item styled with error text.
 */
.dropdown-menu {
    position: fixed;
    z-index: var(--z-dropdown);
    min-width: 160px;
    padding: var(--space-2) 0;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
    display: flex;
    flex-direction: column;
    overflow: hidden;
}

.dropdown-item {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    width: 100%;
    height: 24px;
    padding: 0 var(--space-5);
    border: 0;
    background: transparent;
    color: var(--text);
    font-size: var(--text-md);
    font-family: inherit;
    text-align: left;
    cursor: pointer;
}

.dropdown-item:hover,
.dropdown-item.is-active,
.dropdown-item:focus-visible {
    background: var(--hover-overlay);
    outline: none;
}

.dropdown-item-icon {
    color: var(--text-dim);
    font-size: var(--text-sm);
    width: 16px;
    text-align: center;
    flex-shrink: 0;
}

.dropdown-item.is-danger {
    color: var(--error);
}

.dropdown-item.is-danger .dropdown-item-icon {
    color: var(--error);
}

.dropdown-item-meta {
    margin-left: auto;
    color: var(--text-dim);
    font-size: var(--text-sm);
    flex-shrink: 0;
}

.dropdown-divider {
    height: 1px;
    border: 0;
    margin: var(--space-2) 0;
    background: var(--border);
}
```

- [ ] **Step 2: Write `ui/dropdown.js`**

```javascript
// frontend/static/app/ui/dropdown.js
/*
 * Dropdown helper.
 *
 * Renders a floating .dropdown-menu anchored to a trigger element. Items
 * accept { id, label, icon?, danger?, divider? }. Menu closes on
 * selection, Esc, or click outside.
 */
(function initDropdown(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});
    const PLACEMENTS = ['bottom-start', 'bottom-end', 'top-start', 'top-end'];

    function dropdown(trigger, opts) {
        if (!trigger) throw new Error('PickleUI.dropdown: trigger required');
        const itemsSource = opts && opts.items;
        const onSelect = (opts && opts.onSelect) || (() => {});
        const placement = PLACEMENTS.includes(opts && opts.placement) ? opts.placement : 'bottom-start';

        function resolveItems() {
            const resolved = typeof itemsSource === 'function' ? itemsSource() : itemsSource;
            return Array.isArray(resolved) ? resolved : [];
        }

        let menu = null;
        let outsideHandler = null;
        let escHandler = null;

        function close() {
            if (!menu) return;
            menu.remove();
            menu = null;
            if (trigger.setAttribute) trigger.setAttribute('aria-expanded', 'false');
            if (outsideHandler) {
                global.document.removeEventListener('mousedown', outsideHandler, true);
                outsideHandler = null;
            }
            if (escHandler) {
                global.document.removeEventListener('keydown', escHandler, true);
                escHandler = null;
            }
        }

        function open() {
            if (menu) return;
            const doc = global.document;
            menu = doc.createElement('div');
            menu.classList.add('dropdown-menu');
            menu.setAttribute('role', 'menu');

            for (const it of resolveItems()) {
                if (it && it.divider) {
                    const div = doc.createElement('div');
                    div.classList.add('dropdown-divider');
                    menu.appendChild(div);
                    continue;
                }
                const item = doc.createElement('button');
                item.setAttribute('type', 'button');
                item.setAttribute('role', 'menuitem');
                item.classList.add('dropdown-item');
                if (it.danger) item.classList.add('is-danger');
                if (it.icon) {
                    const ic = doc.createElement('span');
                    ic.classList.add('dropdown-item-icon');
                    ic.textContent = it.icon;
                    item.appendChild(ic);
                }
                const label = doc.createElement('span');
                label.textContent = it.label;
                item.appendChild(label);
                if (it.meta) {
                    const metaSpan = doc.createElement('span');
                    metaSpan.classList.add('dropdown-item-meta');
                    metaSpan.textContent = it.meta;
                    item.appendChild(metaSpan);
                }
                item.addEventListener('click', (event) => {
                    event.stopPropagation();
                    close();
                    onSelect(it.id);
                });
                menu.appendChild(item);
            }

            // Position the menu.
            const rect = trigger.getBoundingClientRect();
            menu.style.position = 'fixed';
            menu.style.minWidth = Math.max(rect.width, 160) + 'px';
            const wantTop = placement.startsWith('top');
            const alignEnd = placement.endsWith('end');
            if (wantTop) {
                menu.style.bottom = (global.innerHeight - rect.top + 4) + 'px';
            } else {
                menu.style.top = (rect.bottom + 4) + 'px';
            }
            if (alignEnd) {
                menu.style.right = (global.innerWidth - rect.right) + 'px';
            } else {
                menu.style.left = rect.left + 'px';
            }

            doc.body.appendChild(menu);
            if (trigger.setAttribute) trigger.setAttribute('aria-expanded', 'true');

            outsideHandler = (event) => {
                if (!trigger.contains(event.target) && !menu.contains(event.target)) close();
            };
            escHandler = (event) => { if (event.key === 'Escape') close(); };
            doc.addEventListener('mousedown', outsideHandler, true);
            doc.addEventListener('keydown', escHandler, true);
        }

        if (trigger.addEventListener) {
            trigger.setAttribute && trigger.setAttribute('aria-haspopup', 'menu');
            trigger.setAttribute && trigger.setAttribute('aria-expanded', 'false');
            trigger.addEventListener('click', (event) => {
                event.preventDefault();
                if (menu) close(); else open();
            });
        }

        return { open, close };
    }

    PickleUI.dropdown = dropdown;
})(typeof window !== 'undefined' ? window : globalThis);
```

- [ ] **Step 3: Wire imports**

`style.css`:
```css
@import url("styles/components/dropdown-menu.css");
```

`index.html` (before `ui/form.js` so `form.js` can depend on it):
```html
<script src="static/app/ui/00-namespace.js"></script>
<script src="static/app/ui/dropdown.js"></script>
<script src="static/app/ui/form.js"></script>
```

- [ ] **Step 4: Run tests**

Run: `node --test frontend/tests/ui/dropdown.test.js`
Expected: PASS.

### Task 5.3: Refactor `ui/form.js` to delegate select popover to `PickleUI.dropdown`

**Files:**
- Modify: `frontend/static/app/ui/form.js`
- Modify: `frontend/tests/ui/form.test.js`

- [ ] **Step 1: Replace the inline popover with a dropdown call**

```javascript
// frontend/static/app/ui/form.js (revised)
(function initForm(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});

    function select(trigger, opts) {
        if (!trigger) throw new Error('PickleUI.select: trigger required');
        const options = Array.isArray(opts && opts.options) ? opts.options : [];
        const onSelect = (opts && opts.onSelect) || (() => {});
        const placement = (opts && opts.placement) === 'top' ? 'top-start' : 'bottom-start';

        trigger.classList.add('select-trigger');
        trigger.setAttribute('aria-haspopup', 'listbox');
        trigger.setAttribute('aria-expanded', 'false');
        let current = null;

        function labelFor(value) {
            const m = options.find((o) => o.value === value);
            return m ? m.label : '';
        }

        if (!trigger.textContent && options[0]) trigger.textContent = options[0].label;

        PickleUI.dropdown(trigger, {
            items: options.map((o) => ({ id: o.value, label: o.label, icon: o.icon })),
            placement,
            onSelect: (value) => {
                current = value;
                trigger.textContent = labelFor(value) || trigger.textContent;
                onSelect(value);
            },
        });

        return {
            setValue(value) { current = value; trigger.textContent = labelFor(value) || trigger.textContent; },
            getValue() { return current; },
        };
    }

    PickleUI.select = select;
})(typeof window !== 'undefined' ? window : globalThis);
```

- [ ] **Step 2: Update `form.test.js` loader**

The form tests load `ui/00-namespace.js` + `ui/form.js`. After this refactor, `form.js` depends on `ui/dropdown.js`. Update the `loadForm()` helper to concat dropdown.js between namespace and form:

```javascript
function loadForm() {
    const namespace = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const dropdown = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'dropdown.js'), 'utf8');
    const form = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'form.js'), 'utf8');
    return namespace + '\n' + dropdown + '\n' + form;
}
```

No test assertions change — outside-click and Escape close still come from the (now delegated) dropdown layer, which sets `aria-expanded="false"` on the trigger just like the old inline popover did.

- [ ] **Step 3: Run tests**

Run: `node --test frontend/tests/ui/form.test.js frontend/tests/ui/dropdown.test.js`
Expected: PASS for both.

### Task 5.4: Migrate package-actions menu to `PickleUI.dropdown`

**Files:**
- Modify: `frontend/index.html`
- Modify: `frontend/static/app/06-shell.js`

- [ ] **Step 1: Strip the hardcoded menu from the HTML**

```html
<div id="pkg-control-group" class="package-split" style="display:none">
    <select id="pkg-select" aria-label="Package"></select>
    <button id="pkg-menu-btn" type="button" class="btn btn-icon btn-ghost"
            aria-haspopup="menu" aria-expanded="false" aria-label="Package actions"
            title="Package actions">...</button>
</div>
```

Remove `#pkg-menu`, `#pkg-edit-name-btn`, `#pkg-reset-name-btn`, `#pkg-delete-btn` — the dropdown renders them at runtime.

- [ ] **Step 2: Replace wiring in `06-shell.js`**

Delete `closePackageMenu`, `refreshPackageMenuState`, `wirePackageMenu`, and `packageMenuBound`. Replace the call site (`wirePackageMenu()` in bootstrap sequencing) with a single `PickleUI.dropdown` instance that uses a factory so items reflect current overlay/override state on every open.

Note: in this file `$` is the usual `document.getElementById` helper. Labels come from `appConfig.ui.packageManager`; the existing helpers `selectedPackageIsOverlay()` and `hasPackageDisplayNameOverride(deviceData.selected_package, selectedPackageMeta())` stay.

```javascript
let packageMenuDropdown = null;

function wirePackageMenu() {
    if (packageMenuDropdown) return;
    const button = $('pkg-menu-btn');
    if (!button) return;

    const ui = appConfig.ui.packageManager;
    button.textContent = ui.menuButtonLabel;
    button.title = ui.menuButtonTitle;
    button.setAttribute('aria-label', ui.menuButtonTitle);

    packageMenuDropdown = window.PickleUI.dropdown(button, {
        placement: 'bottom-end',
        items: () => {
            const items = [{ id: 'edit', label: ui.menuEditLabel }];
            if (hasPackageDisplayNameOverride(deviceData?.selected_package, selectedPackageMeta())) {
                items.push({ id: 'reset', label: ui.menuResetLabel });
            }
            if (selectedPackageIsOverlay()) {
                items.push({ divider: true });
                items.push({ id: 'delete', label: ui.menuDeleteLabel, danger: true });
            }
            return items;
        },
        onSelect: (id) => {
            if (id === 'edit') return showPackageManagerDialog();
            if (id === 'reset') return void resetSelectedPackageDisplayName();
            if (id === 'delete') return void deleteSelectedOverlayPackage();
        },
    });
}
```

The trigger's `group` visibility was previously driven by `refreshPackageMenuState`. Its one remaining job — hiding `#pkg-control-group` when no package is selected — moves to the caller that currently invokes `refreshPackageMenuState()`. Grep for `refreshPackageMenuState(` and replace each call site with a plain `hideElement($('pkg-control-group'))` / `showElement($('pkg-control-group'))` based on `Boolean(deviceData?.selected_package)`. The factory handles everything else on open.

### Task 5.5: Migrate save menu to `PickleUI.dropdown`

**Files:**
- Modify: `frontend/index.html`
- Modify: `frontend/static/app/06-shell.js` (or wherever `#save-menu-btn` wiring lives)

- [ ] **Step 1: Strip the hardcoded menu**

```html
<div id="save-action-group" class="save-action-group" style="display:none">
    <div class="save-split">
        <button id="save-btn" class="btn btn-primary"
                title="Save current configuration (Ctrl+S)">Save Config</button>
        <button id="save-menu-btn" type="button" class="btn btn-icon btn-primary"
                aria-haspopup="menu" aria-expanded="false" title="More save actions">▾</button>
    </div>
</div>
```

Remove `#save-menu`, `#save-as-btn`, `#rename-btn`.

- [ ] **Step 2: Wire**

Delete `closeSaveMenu`, `toggleSaveMenu`, and `saveMenuBound`. Replace `wireSaveMenu`'s body — the existing `bindClick('save-menu-btn', ...)` toggle + `bindClick('save-as-btn', ...)` / `bindClick('rename-btn', ...)` + the `document.addEventListener('click'/'keydown', ...)` handlers all go away.

Since save menu labels are static, use a static items array:

```javascript
let saveMenuDropdown = null;

function wireSaveMenu() {
    if (saveMenuDropdown) return;
    const button = $('save-menu-btn');
    if (!button) return;
    saveMenuDropdown = window.PickleUI.dropdown(button, {
        placement: 'bottom-end',
        items: [
            { id: 'save-as', label: 'Save As...' },
            { id: 'rename', label: 'Rename...' },
        ],
        onSelect: (id) => {
            if (id === 'save-as') return runShellAction('save_as');
            if (id === 'rename') return runShellAction('rename');
        },
    });
}
```

### Task 5.6: Migrate part picker suggestion list to `PickleUI.dropdown`

**Files:**
- Modify: `frontend/static/app/06-shell.js`
- Modify: `frontend/index.html`

**Scope note:** the current part picker implements keyboard navigation (ArrowUp/Down/Enter/Tab within the suggestion list). Per spec §Future Work (Lane B), custom-dropdown keyboard navigation is deferred — this migration loses ArrowUp/Down/Tab-to-accept behavior. Enter (submit-to-load) must keep working because it's on the input itself, not the suggestion list. Click-to-select autocomplete and the cached badge must survive via `item.meta`.

- [ ] **Step 1: Delete the hidden `#part-suggestions` container and rewrite the helpers**

Remove from `index.html` (line 25):
```html
<div id="part-suggestions" class="part-suggestions" role="listbox" hidden></div>
```

Also remove the `aria-controls="part-suggestions"` / `aria-expanded="false"` / `aria-autocomplete="list"` attributes from `#part-input` since the dropdown sets its own `aria-expanded`.

In `06-shell.js`, delete these helpers and the module-scoped state they touch (`visiblePartSuggestions`, `activePartSuggestionIndex`):

- `hidePartSuggestions`
- `setActivePartSuggestion`
- `applyPartSuggestion`
- `renderPartSuggestions`
- `updatePartSuggestions`
- `handlePartPickerKeydown`
- the existing `wirePartPicker`

Keep `rankDeviceSuggestion` and `findPartSuggestions` (they ship the matching logic) and `cachedDevices` / `catalogDeviceNames` references.

- [ ] **Step 2: Rewrite `wirePartPicker` to use `PickleUI.dropdown`**

```javascript
let partPickerDropdown = null;
let lastPartSuggestions = [];

function wirePartPicker() {
    const input = $('part-input');
    if (!input || partPickerDropdown) return;

    const cachedLabel = appConfig.ui.partPicker.cachedLabel;
    partPickerDropdown = window.PickleUI.dropdown(input, {
        placement: 'bottom-start',
        items: () => lastPartSuggestions.map((deviceName) => ({
            id: deviceName,
            label: deviceName,
            meta: cachedDevices.has(deviceName) ? cachedLabel : undefined,
        })),
        onSelect: (deviceName) => {
            if (!deviceName) return;
            input.value = deviceName;
            input.focus();
        },
    });

    function refresh() {
        const normalizedValue = input.value.toUpperCase();
        if (normalizedValue !== input.value) input.value = normalizedValue;
        lastPartSuggestions = findPartSuggestions(normalizedValue);
        partPickerDropdown.close();
        if (lastPartSuggestions.length) partPickerDropdown.open();
    }

    input.addEventListener('input', refresh);
    input.addEventListener('focus', refresh);
    input.addEventListener('keydown', (event) => {
        if (event.key === 'Enter') {
            dismissWelcomeIntro({ persist: true });
            void loadDevice();
        }
    });
}
```

`findPartSuggestions` already applies `maxSuggestions` capping, so no extra slicing. The dropdown's click-outside handler replaces the old `document.addEventListener('click', ...)` close behavior, and Escape handling comes from the dropdown primitive itself.

### Task 5.7: Delete legacy dropdown CSS

**Files:**
- Modify: `frontend/static/styles/04-shell-layout.css`

- [ ] **Step 1: Remove rules**

Delete the `.package-menu`, `.package-menu-item`, `.save-menu`, `.save-menu-item`, `.part-suggestions`, `.part-suggestion`, `.part-suggestion-part`, `.part-suggestion-meta` rule blocks (including their `:hover`, `.is-active`, `[hidden]`, and `:disabled` variants). All currently live in `04-shell-layout.css`.

Also remove the feature-level hook classes from `index.html`: `class="btn ... package-menu-btn"` → `class="btn ..."` on `#pkg-menu-btn`, and `class="btn ... save-menu-btn"` → `class="btn ..."` on `#save-menu-btn`. The dropdown primitive needs no feature-level hook.

- [ ] **Step 2: Sanity gate**

CSS — no surviving legacy class rules:
```bash
rg '\.(package-menu|save-menu|part-suggestion)' frontend/static/styles/
```
HTML — no legacy hook classes or container ids remain (trigger `#pkg-menu-btn` / `#save-menu-btn` are kept):
```bash
rg '\b(package-menu|save-menu|part-suggestions?)\b' frontend/index.html | rg -v '(pkg|save)-menu-btn'
```
Both expected: zero matches.

### Task 5.8: Commit PR #5

- [ ] **Step 1: Validate**

Run: `scripts/validate.sh`
Expected: PASS.

- [ ] **Step 2: Launch + smoke**

Run: `cargo tauri dev`. Exercise package menu, save menu, part picker autocomplete. Confirm keyboard focus behavior still works (no regressions from the menu wrapper change).

- [ ] **Step 3: Commit**

```bash
git add frontend/static/styles/components/dropdown-menu.css frontend/static/app/ui/dropdown.js frontend/tests/ui/dropdown.test.js frontend/static/style.css frontend/index.html frontend/static/app/ui/form.js frontend/static/app/06-shell.js frontend/static/styles/04-shell-layout.css
git commit -m "$(cat <<'EOF'
Lane A: dropdown menu primitive

- Add components/dropdown-menu.css + ui/dropdown.js with items factory, item.meta, divider, danger, placement
- Refactor ui/form.js (PickleUI.select) to delegate popover to PickleUI.dropdown
- Migrate package actions menu, save menu, part-picker suggestions
- Delete .package-menu, .save-menu, .part-suggestion CSS
EOF
)"
```

- [ ] **Step 4: Append logbook entry**

`Lane A PR #5: dropdown primitive landed. Migrated package menu, save menu, part picker. Removed ad-hoc menu markup and CSS.`

---

## PR #6 — Tab strip primitive

Adds `components/tab-strip.css` + `ui/tab-strip.js`. Migrates the right-panel 5-tab strip (Info/Fuses/CLC/Code/Verify), the Pin/Peripheral toggle, and the CLC module tab strip.

### Task 6.1: Tab strip — tests

**Files:**
- Create: `frontend/tests/ui/tab-strip.test.js`

- [ ] **Step 1: Write the failing test**

```javascript
// frontend/tests/ui/tab-strip.test.js
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function load() {
    const ns = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const ts = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'tab-strip.js'), 'utf8');
    return ns + '\n' + ts;
}

function makeContainer(ids, activeId) {
    const children = ids.map((id) => {
        const cl = new Set(['tab-strip-item']);
        if (id === activeId) cl.add('is-active');
        const l = {};
        const el = {
            tagName: 'BUTTON',
            dataset: { tabId: id },
            attributes: {},
            classList: { add: (c) => cl.add(c), remove: (c) => cl.delete(c), contains: (c) => cl.has(c), _set: cl },
            setAttribute(n, v) { this.attributes[n] = v; },
            getAttribute(n) { return this.attributes[n]; },
            addEventListener(t, fn) { (l[t] ||= []).push(fn); },
            click() { for (const fn of (l.click || [])) fn({ target: el, preventDefault() {} }); },
        };
        return el;
    });
    return {
        classList: { add: () => {}, contains: () => false },
        children,
        querySelectorAll(sel) {
            if (sel === '.tab-strip-item') return children;
            return [];
        },
    };
}

test('PickleUI.tabStrip sets aria roles and initial active state', () => {
    const source = load();
    const container = makeContainer(['a', 'b', 'c'], 'b');
    const sandbox = { window: {} };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.tabStrip(container, { onChange: () => {} });
    assert.equal(container.children[0].attributes.role, 'tab');
    assert.equal(container.children[1].attributes['aria-selected'], 'true');
    assert.equal(container.children[0].attributes['aria-selected'], 'false');
});

test('PickleUI.tabStrip fires onChange and toggles is-active', () => {
    const source = load();
    const container = makeContainer(['a', 'b', 'c'], 'a');
    const seen = [];
    const sandbox = { window: {} };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.tabStrip(container, { onChange: (id) => seen.push(id) });
    container.children[2].click();
    assert.deepEqual(seen, ['c']);
    assert.equal(container.children[0].classList.contains('is-active'), false);
    assert.equal(container.children[2].classList.contains('is-active'), true);
});

test('PickleUI.tabStrip.activate can programmatically select', () => {
    const source = load();
    const container = makeContainer(['a', 'b'], 'a');
    const seen = [];
    const sandbox = { window: {} };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const handle = sandbox.window.PickleUI.tabStrip(container, { onChange: (id) => seen.push(id) });
    handle.activate('b', { silent: true });
    assert.equal(container.children[1].classList.contains('is-active'), true);
    assert.deepEqual(seen, []);
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --test frontend/tests/ui/tab-strip.test.js`
Expected: FAIL.

### Task 6.2: Tab strip — CSS + helper

**Files:**
- Create: `frontend/static/styles/components/tab-strip.css`
- Create: `frontend/static/app/ui/tab-strip.js`
- Modify: `frontend/static/style.css`
- Modify: `frontend/index.html`

- [ ] **Step 1: Write `tab-strip.css`**

```css
/*
 * Tab strip primitive.
 *
 * Two variants:
 *   - underline (default): bottom border, active cell gets a 2 px accent underline
 *   - segmented: bordered group, active cell has elevated card bg fill
 *
 * Apply .tab-strip-segmented modifier on the container to switch style.
 * Keyboard navigation (arrows/Home/End) is Lane B scope — this primitive
 * only handles click + aria-selected.
 */
.tab-strip {
    display: flex;
    border-bottom: 1px solid var(--border);
    gap: 0;
}

.tab-strip-item {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: var(--control-h-md);
    padding: 0 var(--space-7);
    border: 0;
    background: transparent;
    color: var(--text-dim);
    font-size: var(--text-md);
    font-weight: var(--weight-regular);
    cursor: pointer;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    transition: color var(--motion-fast) ease, border-color var(--motion-fast) ease;
}

.tab-strip-item:hover {
    color: var(--text);
}

.tab-strip-item.is-active {
    color: var(--text);
    font-weight: var(--weight-medium);
    border-bottom-color: var(--accent);
}

.tab-strip-item:focus-visible {
    outline: var(--focus-ring-width) solid var(--focus-ring-color);
    outline-offset: calc(var(--focus-ring-offset) * -1);
}

.tab-strip-item[disabled],
.tab-strip-item.is-disabled {
    opacity: 0.4;
    cursor: not-allowed;
}

.tab-strip-segmented {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
    border-bottom: 1px solid var(--border);
}

.tab-strip-segmented .tab-strip-item {
    height: var(--control-h-md);
    padding: 0 var(--space-7);
    border-right: 1px solid var(--border);
    border-bottom: 0;
    margin-bottom: 0;
}

.tab-strip-segmented .tab-strip-item:last-child {
    border-right: 0;
}

.tab-strip-segmented .tab-strip-item.is-active {
    background: var(--hover-overlay);
    color: var(--text);
    font-weight: var(--weight-medium);
    border-bottom-color: transparent;
}
```

- [ ] **Step 2: Write `ui/tab-strip.js`**

```javascript
// frontend/static/app/ui/tab-strip.js
/*
 * Tab strip helper.
 *
 * Given a container with .tab-strip-item children that each carry
 * [data-tab-id], wires click -> aria-selected toggle + onChange callback.
 * Programmatic .activate(id) is available for keyboard shortcuts and for
 * initial state restoration.
 */
(function initTabStrip(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});

    function tabStrip(container, opts) {
        if (!container) throw new Error('PickleUI.tabStrip: container required');
        const onChange = (opts && opts.onChange) || (() => {});
        const items = Array.from(container.querySelectorAll('.tab-strip-item'));

        container.classList.add('tab-strip');
        container.setAttribute && container.setAttribute('role', 'tablist');

        for (const item of items) {
            item.setAttribute('role', 'tab');
            const selected = item.classList.contains('is-active');
            item.setAttribute('aria-selected', selected ? 'true' : 'false');
            item.setAttribute('tabindex', selected ? '0' : '-1');
            item.addEventListener('click', (event) => {
                event.preventDefault();
                if (item.classList.contains('is-disabled') || item.getAttribute('aria-disabled') === 'true') return;
                activate(item.dataset.tabId);
            });
        }

        function activate(id, flags) {
            const silent = !!(flags && flags.silent);
            let changed = false;
            for (const item of items) {
                const active = item.dataset.tabId === id;
                const was = item.classList.contains('is-active');
                if (active !== was) changed = true;
                if (active) item.classList.add('is-active'); else item.classList.remove('is-active');
                item.setAttribute('aria-selected', active ? 'true' : 'false');
                item.setAttribute('tabindex', active ? '0' : '-1');
            }
            if (changed && !silent) onChange(id);
        }

        return {
            activate,
            current() {
                const match = items.find((i) => i.classList.contains('is-active'));
                return match ? match.dataset.tabId : null;
            },
        };
    }

    PickleUI.tabStrip = tabStrip;
})(typeof window !== 'undefined' ? window : globalThis);
```

- [ ] **Step 3: Wire imports**

`style.css`:
```css
@import url("styles/components/tab-strip.css");
```

`index.html`:
```html
<script src="static/app/ui/tab-strip.js"></script>
```

- [ ] **Step 4: Run tests**

Run: `node --test frontend/tests/ui/tab-strip.test.js`
Expected: PASS.

### Task 6.3: Migrate right-panel 5-tab strip

**Files:**
- Modify: `frontend/index.html` (tab buttons + 5 content-pane wrappers)
- Modify: `frontend/static/app/06-shell.js` (click wiring + `switchRightTab` body, currently at lines 387-412)
- Modify: `frontend/static/app/05-clc-designer.js` (CLC-disable selector, currently at lines 60 and 70)

> **Rename note:** The shell's existing helper is `switchRightTab(tabName)` — NOT `showRightTabContent`. Both the buttons and their matching content panes currently use `data-tab`; this task renames that attribute to `data-tab-id` on both sides so the tab-strip primitive's `dataset.tabId` lookups work uniformly with the other strips in Tasks 6.4 and 6.5. The `.right-tab-content` class itself and its `.right-tab-content.active { display: flex; }` display rule stay (they are not part of the primitive).

- [ ] **Step 1: Update markup**

Swap button classes (`right-tab active` → `tab-strip-item is-active`) and rename `data-tab` → `data-tab-id` on the five buttons:

```html
<div class="tab-strip" id="right-tabs" role="tablist">
    <button class="tab-strip-item is-active" data-tab-id="info">Info</button>
    <button class="tab-strip-item" data-tab-id="fuses">Fuses</button>
    <button class="tab-strip-item" data-tab-id="clc">CLC</button>
    <button class="tab-strip-item" data-tab-id="code">Code</button>
    <button class="tab-strip-item" data-tab-id="verify">Verification</button>
</div>
```

Rename `data-tab` → `data-tab-id` on the five `.right-tab-content` panels as well (lines 119, 124, 162, 195, 209 of index.html). Keep their `.right-tab-content` class; only the attribute changes.

- [ ] **Step 2: Replace click-binding block and update `switchRightTab`**

At `06-shell.js:387-389` replace the existing click-binding loop with:

```javascript
window.PickleUI.tabStrip(document.getElementById('right-tabs'), {
    onChange: (id) => switchRightTab(id),
});
```

Rewrite the `switchRightTab` body (currently `06-shell.js:397-412`) so the button-state toggle reads the new class/attribute pair. Content panes still flip their `.active` class:

```javascript
function switchRightTab(tabName) {
    const targetTab = document.querySelector(`.tab-strip-item[data-tab-id="${tabName}"]`);
    if (targetTab?.disabled || targetTab?.classList.contains('is-disabled')) {
        if (tabName === 'clc' && typeof setStatus === 'function') {
            setStatus('This device has no CLC peripheral.', 'idle');
        }
        return;
    }

    document.querySelectorAll('#right-tabs .tab-strip-item').forEach((tab) => {
        tab.classList.toggle('is-active', tab.dataset.tabId === tabName);
    });
    document.querySelectorAll('.right-tab-content').forEach((content) => {
        content.classList.toggle('active', content.dataset.tabId === tabName);
    });
}
```

Update the CLC-disable lookup in `05-clc-designer.js` (currently `.right-tab[data-tab="clc"]` at line 60 and `.active` class check at line 70):

```javascript
const tab = document.querySelector('.tab-strip-item[data-tab-id="clc"]');
// ...
if (disabled && tab.classList.contains('is-active') && typeof switchRightTab === 'function') {
    switchRightTab('info');
}
```

`07-verification.js` calls `switchRightTab('verify'|'code')` by name — no changes needed there.

### Task 6.4: Migrate Pin/Peripheral view toggle (segmented)

**Files:**
- Modify: `frontend/index.html` (view-toggle markup at lines 87-88)
- Modify: `frontend/static/app/06-shell.js` (click wiring at lines 391-393 + `switchView` body at lines 415-435)

> **Rename note:** The shell's existing helper is `switchView(viewName)` — NOT `setView` — and it lives in `06-shell.js`, not `02-view-model.js`. The buttons currently carry `btn btn-sm btn-ghost view-toggle-btn` classes plus a `data-view` attribute; this task drops all four of those classes (the segmented primitive styles them now) and renames `data-view` → `data-tab-id`.

- [ ] **Step 1: Update markup**

Replace the two buttons' markup (lines 87-88 of index.html):

```html
<div id="view-toggle" class="tab-strip tab-strip-segmented" style="display:none" role="tablist">
    <button class="tab-strip-item is-active" data-tab-id="pin">Pin View</button>
    <button class="tab-strip-item" data-tab-id="peripheral">Peripheral View</button>
</div>
```

- [ ] **Step 2: Wire the primitive and rewrite `switchView`**

Replace the click-binding loop at `06-shell.js:391-393` with:

```javascript
window.PickleUI.tabStrip(document.getElementById('view-toggle'), {
    onChange: (id) => switchView(id),
});
```

Rewrite the `switchView` body (currently `06-shell.js:415-435`) so the button-state toggle reads the new class/attribute pair. The container/body rendering logic (`activeView`, pin vs peripheral containers, `renderPeripheralView` / `renderDevice` calls) stays unchanged:

```javascript
function switchView(viewName) {
    activeView = viewName;

    document.querySelectorAll('#view-toggle .tab-strip-item').forEach((button) => {
        button.classList.toggle('is-active', button.dataset.tabId === viewName);
    });

    const pinContainer = $('pin-view-container');
    const periphContainer = $('periph-view-container');

    if (viewName === 'peripheral') {
        hideElement(pinContainer);
        showElement(periphContainer);
        renderPeripheralView();
        return;
    }

    hideElement(periphContainer);
    showElement(pinContainer);
    renderDevice();
}
```

### Task 6.5: Migrate CLC module tab strip

**Files:**
- Modify: `frontend/static/app/05-clc-designer.js` (`renderClcModuleTabs` at lines 127-147)

> **Rename note:** There is no `selectModule` helper. `renderClcModuleTabs` currently attaches a per-button click handler inline (`clcActiveModule = i; renderClcDesigner();`); this task moves that body into the primitive's `onChange`. Module ids are 1-indexed (CLC1-CLC6) in the current code and remain so post-migration. The `.clc-tab-dot` configured-state indicator stays — only the container/button classes change.

- [ ] **Step 1: Rewrite `renderClcModuleTabs`**

Replace the whole function body (`05-clc-designer.js:127-147`):

```javascript
function renderClcModuleTabs() {
    const container = document.getElementById('clc-module-tabs');
    container.innerHTML = '';

    for (let i = 1; i <= getClcModuleCount(); i++) {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.classList.add('tab-strip-item');
        if (i === clcActiveModule) btn.classList.add('is-active');
        btn.dataset.tabId = String(i);
        btn.textContent = 'CLC' + i;
        if (isClcModuleConfigured(i)) {
            const dot = document.createElement('span');
            dot.className = 'clc-tab-dot';
            btn.appendChild(dot);
        }
        container.appendChild(btn);
    }

    window.PickleUI.tabStrip(container, {
        onChange: (id) => { clcActiveModule = Number(id); renderClcDesigner(); },
    });
}
```

The primitive adds `.tab-strip` to the container and wires clicks per rebuild. `renderClcDesigner` re-enters `renderClcModuleTabs` on selection, so the rebuild-and-rewire pattern is preserved. The container's `clc-module-tabs` class stays on the element (id + class are kept) so sizing rules scoped to `.clc-module-tabs` survive, and the tab-strip primitive adds its own `.tab-strip` class alongside.

### Task 6.6: Delete legacy tab CSS

**Files:**
- Modify: `frontend/static/styles/03-verify-clc.css`
- Modify: `frontend/static/styles/04-shell-layout.css`
- Modify: `frontend/static/styles/05-peripheral-responsive.css` (the `.view-toggle-btn` rules actually live here — lines 12-32 — NOT in 04-shell-layout.css; the plan originally missed this file)

> **Keep notes:**
> - `.right-tab-content` / `.right-tab-content.active` / `.fuses-tab-content` (in 03-verify-clc.css) stay — they control content-pane display, not the tab bar.
> - `.clc-module-tabs` container rule (03-verify-clc.css:247-253) stays — it provides the wrap behavior required for AK parts with >4 CLC modules (see 2026-04-09 logbook entry). The tab-strip primitive adds `.tab-strip` onto the same element, and because `components/tab-strip.css` imports before `03-verify-clc.css`, the wrap rule still wins.
> - `.clc-tab-dot` survives, but its selector (`.clc-module-tab .clc-tab-dot`) no longer matches post-migration — rescope to `.clc-module-tabs .clc-tab-dot` below.

- [ ] **Step 1: Delete legacy button rules**

From `03-verify-clc.css`: remove `.right-tabs` (container layout, lines 9-15), `.right-tab`, `.right-tab:hover`, `.right-tab.active`, `.right-tab.is-disabled`, `.right-tab:disabled`, `.right-tab.is-disabled:hover`, `.right-tab:disabled:hover` (lines 16-44); and `.clc-module-tab`, `.clc-module-tab:hover`, `.clc-module-tab.active` (lines 255-275). Rescope the dot selector:

```css
.clc-module-tabs .clc-tab-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--sys);
    margin-left: 5px;
    vertical-align: middle;
}
```

From `05-peripheral-responsive.css`: remove `.view-toggle` (container layout, lines 3-10), `.view-toggle-btn`, `.view-toggle-btn:hover`, `.view-toggle-btn.active` (lines 12-32). Keep `#periph-view` and the `.periph-toolbar*` rules below that block.

From `04-shell-layout.css`: sweep for any duplicates of the above selectors (there shouldn't be any, but confirm).

- [ ] **Step 2: Sanity gate**

```bash
rg '^\s*\.(right-tab|view-toggle|view-toggle-btn|clc-module-tab)(\.|:|\s|\{|$)' frontend/static/styles/
```
Expected: zero matches. (The trailing char class excludes `.right-tab-content` and `.clc-module-tabs` because `-content` / `-tabs` starts with `-`, which isn't in the allowed follow set — those stay in place.)

> **Visual change to verify during smoke test:** The tab-strip primitive uses `--accent` for active-tab color, while the legacy rules used `--accent2` (right tab bar and view toggle) and `--sys` (CLC module tabs). Expect the three strips to shift toward the primary accent. Raise a follow-up if a per-strip accent alias is wanted.

### Task 6.7: Commit PR #6

- [ ] **Step 1: Validate**

Run: `scripts/validate.sh`
Expected: PASS.

- [ ] **Step 2: Smoke**

Run: `cargo tauri dev`. Switch through all five right-panel tabs, toggle Pin/Peripheral view, load a CLC-capable device and click through CLC module tabs. Confirm disabled states and initial-active state are preserved.

- [ ] **Step 3: Commit**

```bash
git add frontend/static/styles/components/tab-strip.css frontend/static/app/ui/tab-strip.js frontend/tests/ui/tab-strip.test.js frontend/static/style.css frontend/index.html frontend/static/app/06-shell.js frontend/static/app/05-clc-designer.js frontend/static/styles/03-verify-clc.css frontend/static/styles/04-shell-layout.css frontend/static/styles/05-peripheral-responsive.css
git commit -m "$(cat <<'EOF'
Lane A: tab strip primitive

- Add components/tab-strip.css (underline + segmented variants)
- Add ui/tab-strip.js (PickleUI.tabStrip) with click wiring + activate()
- Migrate right-panel 5-tab strip, Pin/Peripheral toggle (segmented), CLC module tabs
- Delete .right-tab, .view-toggle-btn, .clc-module-tab CSS
EOF
)"
```

- [ ] **Step 4: Append logbook entry**

`Lane A PR #6: tab strip primitive landed. Migrated three strips (right-panel, view toggle, CLC modules).`

---

## PR #7 — Empty state primitive

Adds `components/empty-state.css`. CSS-led, but also touches two JS files: `07-verification-render.js` (rebuilds the post-verify empty on every render) and `05-clc-designer.js` (writes state-specific copy into `#clc-empty`). Migrates all four placeholder empties (`#device-info-empty`, `#fuses-empty`, `#clc-empty`, `#verify-empty` inside `#verify-output`).

### Task 7.1: Write `components/empty-state.css`

**Files:**
- Create: `frontend/static/styles/components/empty-state.css`
- Modify: `frontend/static/style.css`

- [ ] **Step 1: Write stylesheet**

```css
/*
 * Empty state primitive.
 *
 * Centered column inside a dashed border — icon, heading, body, optional
 * CTA slot. Replaces .verify-empty / .clc-empty / .device-info-empty.
 */
.empty-state {
    padding: var(--space-10) var(--space-10);
    text-align: center;
    border: 1px dashed var(--border);
    border-radius: var(--radius-md);
    color: var(--text-dim);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
}

.empty-state-icon {
    font-size: 28px;
    line-height: 1;
    color: var(--text-dim);
    margin-bottom: var(--space-6);
}

.empty-state-title {
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
    color: var(--text);
    margin: 0 0 var(--space-2);
}

.empty-state-body {
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
    color: var(--text-dim);
    max-width: 40ch;
    margin: 0 auto var(--space-7);
}

.empty-state-action {
    display: inline-flex;
}
```

- [ ] **Step 2: Import**

`style.css`:
```css
@import url("styles/components/empty-state.css");
```

### Task 7.2: Migrate device-info empty

**Files:**
- Modify: `frontend/index.html`

- [ ] **Step 1: Rewrite the Info-tab empty**

```html
<div id="device-info-empty" class="empty-state">
    <div class="empty-state-icon">ⓘ</div>
    <h3 class="empty-state-title">No device loaded</h3>
    <p class="empty-state-body">Load a device to view its information, peripherals, and datasheet metadata.</p>
</div>
```

### Task 7.3: Migrate fuses empty

**Files:**
- Modify: `frontend/index.html`

- [ ] **Step 1: Rewrite**

```html
<div id="fuses-empty" class="empty-state">
    <div class="empty-state-icon">⚙</div>
    <h3 class="empty-state-title">No device loaded</h3>
    <p class="empty-state-body">Load a device to configure oscillator and fuse settings.</p>
</div>
```

### Task 7.4: Migrate CLC empty

**Files:**
- Modify: `frontend/index.html`
- Modify: `frontend/static/app/05-clc-designer.js`

`renderClcDesigner` currently calls `empty.textContent = clcEmptyMessage()` at two spots, which would destroy the new structured children (icon/title/body). And `clcEmptyMessage()` returns three different strings depending on state (no device / no CLC / CLC unparsed), so a single static title hardcoded in the HTML would misdescribe two of those states. Refactor the message helper to return `{title, body}` and have the render function write each into its matching `.empty-state-*` child.

- [ ] **Step 1: Rewrite the HTML scaffold**

```html
<div id="clc-empty" class="empty-state">
    <div class="empty-state-icon">▤</div>
    <h3 class="empty-state-title">No device loaded</h3>
    <p class="empty-state-body">Load a device with CLC peripherals to configure logic modules.</p>
</div>
```

The title/body text here is the *default* for the "no device" state — `renderClcDesigner` will overwrite it on every render.

- [ ] **Step 2: Refactor `clcEmptyMessage()` to return `{title, body}`**

`frontend/static/app/05-clc-designer.js` near line 49:

```javascript
function clcEmptyMessage() {
    if (!deviceData) {
        return {
            title: 'No device loaded',
            body: 'Load a device to configure CLC modules.',
        };
    }
    if (!deviceHasClc()) {
        return {
            title: 'No CLC peripheral',
            body: 'This device has no CLC peripheral. The CLC editor and datasheet CLC lookup are disabled for this part.',
        };
    }
    return {
        title: 'CLC sources unavailable',
        body: 'CLC input sources are not available yet. Verify the datasheet to import them if needed.',
    };
}
```

- [ ] **Step 3: Update `renderClcDesigner` to write into the structured children**

`frontend/static/app/05-clc-designer.js` near line 100 — replace both `empty.textContent = clcEmptyMessage();` sites. The second one (line 114) runs right after the empty is hidden, so it's dead weight — delete it entirely. The first one becomes:

```javascript
if (!deviceData || !deviceHasClc()) {
    designer.style.display = 'none';
    empty.style.display = '';
    const msg = clcEmptyMessage();
    empty.querySelector('.empty-state-title').textContent = msg.title;
    empty.querySelector('.empty-state-body').textContent = msg.body;
    return;
}

designer.style.display = '';
empty.style.display = 'none';
syncClcDesignerState();
```

### Task 7.5: Migrate verify empty

**Files:**
- Modify: `frontend/index.html`
- Modify: `frontend/static/app/07-verification-render.js`

- [ ] **Step 1: Replace the initial HTML**

```html
<div id="verify-output">
    <div class="empty-state">
        <div class="empty-state-icon">✓</div>
        <h3 class="empty-state-title">No verification yet</h3>
        <p class="empty-state-body">Load a device and click <strong>Verify Pinout</strong> to cross-check pin assignments against the datasheet. If the package shows as <strong>default</strong>, that is an EDC fallback and not a real package name.</p>
    </div>
</div>
```

- [ ] **Step 2: Update any `verification-render` helpers that emit `.verify-empty`**

If `07-verification-render.js` rebuilds the empty state as a string, change the template to use `.empty-state` / `.empty-state-title` / `.empty-state-body`. Grep:
```bash
rg '\.verify-empty' frontend/static/app/
```
Edit each occurrence.

### Task 7.6: Delete legacy empty-state CSS

**Files:**
- Modify: `frontend/static/styles/03-verify-clc.css`

Only `.verify-empty` currently has a CSS rule (at `03-verify-clc.css:43-49`). `.clc-empty`, `.device-info-empty`, and `.fuses-empty` were never CSS selectors — just id targets. Keep them in the sanity-gate regex anyway to catch future strays.

- [ ] **Step 1: Remove the `.verify-empty` rule**

Delete the entire `.verify-empty { ... }` block from `03-verify-clc.css`.

- [ ] **Step 2: Sanity gate**

```bash
rg '\.(verify-empty|clc-empty|device-info-empty|fuses-empty)' frontend/
```
Expected: zero matches.

### Task 7.7: Commit PR #7

- [ ] **Step 1: Validate**

Run: `scripts/validate.sh`
Expected: PASS.

- [ ] **Step 2: Smoke**

Run: `cargo tauri dev`. Open the app fresh (no device). All four empty states should render as dashed-border centered blocks with the new typography. Load a device, switch between tabs — empty states hide as before.

- [ ] **Step 3: Commit**

```bash
git add frontend/static/styles/components/empty-state.css frontend/static/style.css frontend/index.html frontend/static/app/05-clc-designer.js frontend/static/app/07-verification-render.js frontend/static/styles/03-verify-clc.css
git commit -m "$(cat <<'EOF'
Lane A: empty state primitive

- Add components/empty-state.css (dashed border, icon, title, body, action slot)
- Migrate device-info, fuses, clc, verification empty states to .empty-state
- Refactor clcEmptyMessage() to {title, body} so state copy maps to the structured scaffold
- Delete the legacy .verify-empty CSS rule
EOF
)"
```

- [ ] **Step 4: Append logbook entry**

`Lane A PR #7: empty state primitive landed. Unified all four empties (info, fuses, CLC, verify).`

---

## PR #8 — Modal primitive + dialog migration

Adds `components/modal.css` + `ui/modal.js` (with `PickleUI.modal.confirm(...)`). Migrates the three existing dialogs (Package, About, Settings). This is the largest PR — the last one in the sequence.

**Known scope notes:**
- Existing per-dialog event wiring (backdrop-click close in `05-settings.js:196`, keydown handlers in `06-shell.js`, etc.) stays in place; the primitive only abstracts `open`/`close`/`confirm`. The `modal.js` helper intentionally does not bake in backdrop-click or Esc handling — native `<dialog>` already handles Esc, and backdrop-click is a per-dialog decision.
- The legacy `.package-dialog`/`.about-dialog`/`.settings-dialog` stylesheets include open animations (opacity/transform + `@starting-style`). The new `.modal` primitive does not reproduce these — dialogs will pop in without animation. This is an accepted minor UX regression for Lane A; if the team wants to reintroduce animation later, add it once in `components/modal.css` rather than per-dialog.
- The codebase uses the `$(id)` shorthand (defined in `00-core.js`) in preference to `document.getElementById(...)`. Greps below account for this.

### Task 8.1: Modal — tests

**Files:**
- Create: `frontend/tests/ui/modal.test.js`

- [ ] **Step 1: Write the failing tests**

```javascript
// frontend/tests/ui/modal.test.js
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function load() {
    const ns = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const m = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'modal.js'), 'utf8');
    return ns + '\n' + m;
}

function makeDialog(id) {
    const cl = new Set(['modal']);
    const listeners = {};
    return {
        id,
        tagName: 'DIALOG',
        attributes: {},
        open: false,
        _focusedAt: null,
        classList: { add: (c) => cl.add(c), remove: (c) => cl.delete(c), contains: (c) => cl.has(c) },
        setAttribute(n, v) { this.attributes[n] = v; },
        getAttribute(n) { return this.attributes[n]; },
        addEventListener(t, fn) { (listeners[t] ||= []).push(fn); },
        removeEventListener(t, fn) { const a = listeners[t] || []; const i = a.indexOf(fn); if (i !== -1) a.splice(i, 1); },
        dispatch(t, ev) { for (const fn of (listeners[t] || [])) fn(ev); },
        showModal() { this.open = true; },
        close() { this.open = false; this.dispatch('close', {}); },
        querySelector() { return null; },
        focus() { this._focusedAt = Date.now(); },
    };
}

test('PickleUI.modal.open calls showModal + restores focus on close', () => {
    const dialog = makeDialog('d1');
    const prior = { focus() { this._restored = true; } };
    const document = {
        getElementById: (id) => (id === 'd1' ? dialog : null),
        activeElement: prior,
    };
    const sandbox = { window: {}, document };
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    sandbox.window.PickleUI.modal.open('d1');
    assert.equal(dialog.open, true);
    dialog.close();
    assert.equal(prior._restored, true);
});

test('PickleUI.modal.close warns when dialog id is unknown', () => {
    const document = { getElementById: () => null, activeElement: null };
    const sandbox = { window: {}, document };
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    // Should not throw.
    sandbox.window.PickleUI.modal.close('does-not-exist');
});

test('PickleUI.modal.confirm resolves true on primary, false on cancel', async () => {
    const sandbox = { window: {}, document: null };
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    // Install a minimal stub document just-in-time when confirm() runs.
    const dialog = makeDialog('pickle-confirm');
    const body = { _children: [], appendChild(el) { this._children.push(el); el.parentNode = this; }, removeChild(el) { const i = this._children.indexOf(el); if (i !== -1) this._children.splice(i, 1); } };
    const doc = {
        body,
        getElementById: (id) => (id === 'pickle-confirm' ? dialog : null),
        activeElement: null,
        createElement: (tag) => {
            const cl = new Set();
            const l = {};
            const el = {
                tagName: tag.toUpperCase(), children: [], textContent: '', attributes: {},
                classList: { add: (c) => cl.add(c), remove: (c) => cl.delete(c), contains: (c) => cl.has(c) },
                appendChild(c) { this.children.push(c); c.parentNode = this; },
                setAttribute(n, v) { this.attributes[n] = v; },
                addEventListener(t, fn) { (l[t] ||= []).push(fn); },
                click() { for (const fn of (l.click || [])) fn({}); },
            };
            if (tag.toLowerCase() === 'dialog') {
                el.showModal = () => { el.open = true; };
                el.close = () => {
                    el.open = false;
                    for (const fn of (l.close || [])) fn({});
                };
            }
            return el;
        },
    };
    sandbox.document = doc;
    sandbox.window.document = doc;

    const p1 = sandbox.window.PickleUI.modal.confirm({
        title: 'Delete?',
        message: 'Really delete?',
        action: 'Delete',
    });
    const createdDialog = body._children[body._children.length - 1];
    // buttons live inside .modal-footer inside createdDialog
    const footer = createdDialog.children.find((c) => c.classList.contains('modal-footer'));
    const cancel = footer.children[0];
    const confirm = footer.children[1];
    confirm.click();
    assert.equal(await p1, true);

    const p2 = sandbox.window.PickleUI.modal.confirm({
        title: 'Delete?',
        message: 'Really delete?',
        action: 'Delete',
    });
    const createdDialog2 = body._children[body._children.length - 1];
    const footer2 = createdDialog2.children.find((c) => c.classList.contains('modal-footer'));
    footer2.children[0].click();
    assert.equal(await p2, false);
});
```

- [ ] **Step 2: Run to verify failure**

Run: `node --test frontend/tests/ui/modal.test.js`
Expected: FAIL.

### Task 8.2: Modal — CSS + helper

**Files:**
- Create: `frontend/static/styles/components/modal.css`
- Create: `frontend/static/app/ui/modal.js`
- Modify: `frontend/static/style.css`
- Modify: `frontend/index.html`

- [ ] **Step 1: Write `modal.css`**

```css
/*
 * Modal primitive.
 *
 * Built on native <dialog>. .modal base handles centering + backdrop;
 * size variants (.modal-sm/md/lg) set width; .modal-with-nav adds the
 * two-column layout used by Settings.
 */
.modal {
    padding: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg);
    color: var(--text);
    box-shadow: var(--shadow-lg);
    min-width: 340px;
    max-width: 90vw;
    max-height: 90vh;
    overflow: hidden;
}

.modal::backdrop {
    background: rgba(0, 0, 0, 0.5);
}

.modal-sm { width: 340px; }
.modal-md { width: 480px; }
.modal-lg { width: 640px; }

.modal-with-nav {
    width: 680px;
    display: grid;
    grid-template-columns: 160px 1fr;
    grid-template-rows: auto 1fr auto;
    grid-template-areas:
        "header header"
        "nav    body"
        "footer footer";
}

.modal-with-nav .modal-header { grid-area: header; }
.modal-with-nav .modal-nav    { grid-area: nav; }
.modal-with-nav .modal-body   { grid-area: body; }
.modal-with-nav .modal-footer { grid-area: footer; }

.modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    padding: var(--space-7) var(--space-8);
    border-bottom: 1px solid var(--border);
    background: var(--bg-card);
}

.modal-title {
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    margin: 0;
}

.modal-subtitle {
    font-size: var(--text-sm);
    color: var(--text-dim);
    margin: var(--space-1) 0 0;
}

.modal-close {
    flex-shrink: 0;
}

.modal-body {
    padding: var(--space-8);
    overflow-y: auto;
}

.modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-4);
    padding: var(--space-6) var(--space-8);
    border-top: 1px solid var(--border);
    background: var(--bg);
}

.modal-nav {
    padding: var(--space-6) var(--space-4);
    border-right: 1px solid var(--border);
    background: var(--bg-card);
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    overflow-y: auto;
}

.modal-nav-item {
    display: flex;
    align-items: center;
    height: 24px;
    padding: 0 var(--space-5);
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-dim);
    font-size: var(--text-md);
    font-family: inherit;
    text-align: left;
    cursor: pointer;
}

.modal-nav-item:hover {
    color: var(--text);
    background: var(--hover-overlay);
}

.modal-nav-item.is-active {
    color: var(--text);
    font-weight: var(--weight-medium);
    background: var(--hover-overlay);
}
```

- [ ] **Step 2: Write `ui/modal.js`**

```javascript
// frontend/static/app/ui/modal.js
/*
 * Modal helper.
 *
 * open(id): showModal() on the <dialog> and remember focused element.
 * close(id): close() on the <dialog>; focus restoration happens on the
 *   dialog's 'close' event, so it also fires for Esc and backdrop-click.
 * confirm(opts): build a small transient dialog, return Promise<boolean>.
 */
(function initModal(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});
    const focusStack = new Map(); // id -> prior focus

    function open(id, opts) {
        const doc = global.document;
        if (!doc) return;
        const dialog = doc.getElementById(id);
        if (!dialog) { console.warn('PickleUI.modal.open: no dialog', id); return; }
        const prior = doc.activeElement || null;
        focusStack.set(id, prior);
        dialog.showModal && dialog.showModal();
        const onClose = () => {
            dialog.removeEventListener('close', onClose);
            const p = focusStack.get(id);
            focusStack.delete(id);
            if (p && typeof p.focus === 'function') p.focus();
            if (opts && typeof opts.onClose === 'function') opts.onClose();
        };
        dialog.addEventListener('close', onClose);
    }

    function close(id) {
        const doc = global.document;
        if (!doc) return;
        const dialog = doc.getElementById(id);
        if (!dialog) { console.warn('PickleUI.modal.close: no dialog', id); return; }
        if (typeof dialog.close === 'function') dialog.close();
    }

    function confirm(opts) {
        const options = opts || {};
        return new Promise((resolve) => {
            const doc = global.document;
            const dialog = doc.createElement('dialog');
            dialog.classList.add('modal');
            dialog.classList.add('modal-sm');

            const header = doc.createElement('div');
            header.classList.add('modal-header');
            const title = doc.createElement('h2');
            title.classList.add('modal-title');
            title.textContent = options.title || 'Confirm';
            header.appendChild(title);
            dialog.appendChild(header);

            const body = doc.createElement('div');
            body.classList.add('modal-body');
            body.textContent = options.message || '';
            dialog.appendChild(body);

            const footer = doc.createElement('div');
            footer.classList.add('modal-footer');
            const cancel = doc.createElement('button');
            cancel.setAttribute('type', 'button');
            cancel.classList.add('btn');
            cancel.classList.add('btn-secondary');
            cancel.textContent = options.cancel || 'Cancel';
            const confirmBtn = doc.createElement('button');
            confirmBtn.setAttribute('type', 'button');
            confirmBtn.classList.add('btn');
            confirmBtn.classList.add('btn-primary');
            if (options.tone === 'danger') confirmBtn.classList.add('btn-danger');
            confirmBtn.textContent = options.action || 'Confirm';
            footer.appendChild(cancel);
            footer.appendChild(confirmBtn);
            dialog.appendChild(footer);

            let settled = false;
            function settle(value) {
                if (settled) return;
                settled = true;
                resolve(value);
                try { dialog.close(); } catch (_) { /* already closed */ }
            }

            cancel.addEventListener('click', () => settle(false));
            confirmBtn.addEventListener('click', () => settle(true));
            dialog.addEventListener('close', () => {
                settle(false);
                if (dialog.parentNode) dialog.parentNode.removeChild(dialog);
            });

            doc.body.appendChild(dialog);
            if (typeof dialog.showModal === 'function') dialog.showModal();
            confirmBtn.focus && confirmBtn.focus();
        });
    }

    PickleUI.modal = { open, close, confirm };
})(typeof window !== 'undefined' ? window : globalThis);
```

- [ ] **Step 3: Wire imports**

`style.css`:
```css
@import url("styles/components/modal.css");
```

`index.html`:
```html
<script src="static/app/ui/modal.js"></script>
```

- [ ] **Step 4: Run tests**

Run: `node --test frontend/tests/ui/modal.test.js`
Expected: PASS.

### Task 8.3: Migrate Package dialog

**Files:**
- Modify: `frontend/index.html`
- Modify: `frontend/static/app/06-shell.js` (or wherever open/close for `#package-dialog` lives)

- [ ] **Step 1: Rewrite Package dialog markup**

```html
<dialog id="package-dialog" class="modal modal-md" aria-label="Edit package name">
    <div class="modal-header">
        <div>
            <h2 id="package-dialog-title" class="modal-title">Edit Package Name</h2>
        </div>
        <button id="package-close-btn" type="button" class="btn btn-icon btn-ghost modal-close"
                aria-label="Close package dialog">✕</button>
    </div>
    <div class="modal-body">
        <div class="field">
            <span class="field-label" id="package-dialog-current-label">Shown in UI</span>
            <div id="package-dialog-current" class="package-dialog-value">—</div>
        </div>
        <div class="field" id="package-dialog-stored-row" hidden>
            <span class="field-label" id="package-dialog-stored-label">Backend key</span>
            <div id="package-dialog-stored" class="package-dialog-value package-dialog-value-mono">—</div>
        </div>
        <div class="field">
            <span class="field-label" id="package-dialog-source-label">Source</span>
            <div id="package-dialog-source" class="package-dialog-value">—</div>
        </div>
        <p id="package-dialog-note" class="field-hint"></p>
        <div class="field">
            <label class="field-label" for="package-name-input" id="package-dialog-name-label">
                Displayed package name
            </label>
            <input id="package-name-input" type="text" class="input"
                   placeholder="Enter the name to show in the UI" autocomplete="off">
        </div>
    </div>
    <div class="modal-footer">
        <button id="package-delete-btn" type="button" class="btn btn-danger">Delete Overlay</button>
        <span style="flex:1"></span>
        <button id="package-cancel-btn" type="button" class="btn btn-secondary">Close</button>
        <button id="package-reset-btn" type="button" class="btn btn-secondary">Reset Name</button>
        <button id="package-save-btn" type="button" class="btn btn-primary">Save Name</button>
    </div>
</dialog>
```

- [ ] **Step 2: Swap open/close calls**

Replace every direct `.showModal()` / `.close()` on the package dialog with `window.PickleUI.modal.open('package-dialog')` / `.close('package-dialog')`. The codebase uses the `$(id)` shorthand (defined in `00-core.js`), not `document.getElementById(...)`, so grep accordingly:

```bash
rg "\\\$\\('package-dialog'\\)" frontend/static/app/
```

Known callsites in `frontend/static/app/00-core.js`:
- `line 588`: inside `openPackageManagerDialog()` — `$('package-dialog').showModal()` — replace with `window.PickleUI.modal.open('package-dialog')`
- `line 665`: inside `syncVerificationPackageDelete()` — reads `dialog.open` first, then calls `.close()` — keep the `dialog.open` read, replace `.close()` with `window.PickleUI.modal.close('package-dialog')`
- `line 677`: `$('package-dialog')?.close()` in `closePackageManagerDialog()` — replace with `window.PickleUI.modal.close('package-dialog')`
- `line 795`: inside `wirePackageManagerDialog()` — gets the dialog for event binding; do NOT replace (still need the element reference for `.addEventListener` calls)

Leave any existing per-dialog event wiring (click listeners, backdrop-click handlers) alone. Focus this task purely on the open/close call sites.

### Task 8.4: Migrate About dialog

**Files:**
- Modify: `frontend/index.html`

- [ ] **Step 1: Rewrite markup**

```html
<dialog id="about-dialog" class="modal modal-md" aria-label="About pickle">
    <div class="modal-body" style="text-align:center;">
        <img src="static/pickle-icon.png" alt="pickle" class="about-icon">
        <h2 class="modal-title">pickle</h2>
        <p class="modal-subtitle">Pin Configurator</p>
        <p class="about-version" id="about-version"></p>
        <p class="about-desc">
            Native desktop pin multiplexing configurator for Microchip
            dsPIC33 and PIC24 devices. Parses Device Family Pack files,
            presents an interactive pin assignment UI, and generates
            MISRA&nbsp;C:2012 compliant initialization code.
        </p>
        <div class="about-tech">
            <span class="about-tag">Rust</span>
            <span class="about-tag">Tauri</span>
            <span class="about-tag">dsPIC33</span>
            <span class="about-tag">PIC24</span>
        </div>
        <div class="about-links">
            <button class="btn btn-secondary" id="about-github-btn">GitHub</button>
        </div>
        <p class="about-copy">&copy; 2026 J. Ihlenburg &middot; GPLv3</p>
        <p class="about-legal">
            Not affiliated with, endorsed by, or approved by Microchip
            Technology&nbsp;Inc. Microchip, dsPIC33, PIC24, and related
            names are trademarks of Microchip Technology&nbsp;Inc.
        </p>
    </div>
    <div class="modal-footer">
        <button class="btn btn-primary" id="about-close-btn">Close</button>
    </div>
</dialog>
```

- [ ] **Step 2: Swap open/close**

Grep:
```bash
rg "\\\$\\('about-dialog'\\)" frontend/static/app/
```

Known callsites in `frontend/static/app/06-shell.js`:
- `line 596`: inside `showAboutDialog()` — `dialog.showModal()` — replace with `window.PickleUI.modal.open('about-dialog')`
- `line 616`: inside `hideAboutDialog()` — `dialog.close()` — replace with `window.PickleUI.modal.close('about-dialog')`

### Task 8.5: Migrate Settings dialog

**Files:**
- Modify: `frontend/index.html`
- Modify: `frontend/static/app/05-settings.js`

- [ ] **Step 1: Rewrite markup (with nav variant)**

```html
<dialog id="settings-dialog" class="modal modal-with-nav" aria-label="Settings">
    <div class="modal-header">
        <h2 class="modal-title">Settings</h2>
        <button id="settings-close-header-btn" type="button"
                class="btn btn-icon btn-ghost modal-close" aria-label="Close">✕</button>
    </div>

    <nav class="modal-nav" id="settings-nav">
        <button class="modal-nav-item is-active" data-section="api-keys">API Keys</button>
        <!-- Future sections: add nav items here -->
    </nav>

    <div class="modal-body" id="settings-content">
        <div class="settings-section is-active" data-section="api-keys">
            <h3 class="field-label" style="font-size:var(--text-lg); text-transform:none;">API Keys</h3>
            <p class="field-hint">
                Keys are stored in your operating system's secure credential
                store (macOS Keychain, Windows Credential Manager, Linux Secret
                Service). Environment variables and <code>.env</code> files are
                still honoured as fallbacks.
            </p>

            <div class="labeled-row" data-setting="verification-provider">
                <label class="labeled-row-label" for="verify-provider-select">Verification Provider</label>
                <p class="labeled-row-desc" id="verify-provider-status">
                    Choose which provider pickle should use for datasheet verification.
                </p>
                <button id="verify-provider-select" type="button" class="btn select-trigger"
                        aria-haspopup="listbox" aria-expanded="false">Auto (prefer OpenAI)</button>
            </div>

            <div class="labeled-row" data-provider="openai">
                <label class="labeled-row-label">OpenAI</label>
                <div class="input-with-action">
                    <input type="password" class="input" id="key-input-openai"
                           placeholder="sk-proj-..." autocomplete="off" spellcheck="false">
                    <button class="btn btn-icon btn-ghost" id="key-reveal-openai" title="Show / hide key">👁</button>
                </div>
                <div style="display:flex; gap:var(--space-3);">
                    <button class="btn btn-sm btn-primary" id="key-save-openai">Save</button>
                    <button class="btn btn-sm btn-secondary" id="key-clear-openai">Clear</button>
                </div>
                <div class="field-hint" id="key-status-openai" style="grid-column:1/-1;"></div>
            </div>

            <div class="labeled-row" data-provider="anthropic">
                <label class="labeled-row-label">Anthropic</label>
                <div class="input-with-action">
                    <input type="password" class="input" id="key-input-anthropic"
                           placeholder="sk-ant-..." autocomplete="off" spellcheck="false">
                    <button class="btn btn-icon btn-ghost" id="key-reveal-anthropic" title="Show / hide key">👁</button>
                </div>
                <div style="display:flex; gap:var(--space-3);">
                    <button class="btn btn-sm btn-primary" id="key-save-anthropic">Save</button>
                    <button class="btn btn-sm btn-secondary" id="key-clear-anthropic">Clear</button>
                </div>
                <div class="field-hint" id="key-status-anthropic" style="grid-column:1/-1;"></div>
            </div>
        </div>
    </div>

    <div class="modal-footer">
        <button class="btn btn-primary" id="settings-close-btn">Done</button>
    </div>
</dialog>
```

- [ ] **Step 2: Update nav-button class names in `05-settings.js`**

Three call sites need the `.settings-nav-btn` → `.modal-nav-item` + `'active'` → `'is-active'` renames. Near `line 43-47` (inside `switchSettingsSection()`):

```javascript
nav.querySelectorAll('.modal-nav-item').forEach((btn) => {
    btn.classList.toggle('is-active', btn.dataset.section === sectionId);
});
content.querySelectorAll('.settings-section').forEach((sec) => {
    sec.classList.toggle('is-active', sec.dataset.section === sectionId);
});
```

Near `line 212` (inside `wireSettingsDialog()`):

```javascript
const btn = e.target.closest('.modal-nav-item');
```

Do NOT touch `.settings-section` as a selector — it remains the section container class. Only the state modifier flips from `.active` to `.is-active`.

- [ ] **Step 3: Swap open/close + wire header close**

In `05-settings.js`:
- `line 176`: `dialog.showModal();` → `window.PickleUI.modal.open('settings-dialog');`
- `line 182`: `dialog.close();` → `window.PickleUI.modal.close('settings-dialog');`
- `line 200`: inside the backdrop-click handler → leave the `dialog.close()` as-is (it's triggered by coordinate math on the same dialog reference; keeping native `.close()` preserves the event chain without a redundant lookup)
- `line 206`: inside Done-button handler → leave as `dialog.close()` (same reason)

Add the new header close button wiring (the `settings-close-header-btn` is introduced in the migrated HTML):

```javascript
const headerCloseBtn = $('settings-close-header-btn');
if (headerCloseBtn) {
    headerCloseBtn.addEventListener('click', () => dialog.close());
}
```

Place this next to the existing Done-button wiring inside `wireSettingsDialog()`.

### Task 8.6: Migrate the "Delete Overlay" confirm to `PickleUI.modal.confirm`

**Files:**
- Modify: `frontend/static/app/00-core.js`

Grep first:
```bash
rg -n 'window\.confirm\(' frontend/static/app/
```

The only hit is inside `deleteSelectedOverlayPackage()` at `00-core.js:769`. The surrounding function is already `async`.

- [ ] **Step 1: Replace the inline `window.confirm(...)` in the delete-overlay flow**

Current code:

```javascript
const packageName = deviceData.selected_package;
const confirmed = window.confirm(appConfig.format(appConfig.ui.packageManager.deleteConfirm, {
    packageName: displayPackageName(packageName, { long: true }),
}));
if (!confirmed) {
    return;
}
```

Replacement:

```javascript
const packageName = deviceData.selected_package;
const confirmed = await window.PickleUI.modal.confirm({
    title: 'Delete overlay?',
    message: appConfig.format(appConfig.ui.packageManager.deleteConfirm, {
        packageName: displayPackageName(packageName, { long: true }),
    }),
    action: 'Delete',
    tone: 'danger',
});
if (!confirmed) {
    return;
}
```

The `deleteSelectedOverlayPackage` function is already `async` (declared at `00-core.js:763`), so no function-signature change is needed.

### Task 8.7: Delete legacy dialog CSS

**Files:**
- Modify: `frontend/static/styles/04-shell-layout.css`

Pre-scan result: all legacy dialog rules live in `04-shell-layout.css` (roughly lines 726–1124). `02-package-config.css` has no `.package-dialog` rules, so it is not in the delete list.

- [ ] **Step 1: Remove legacy dialog rules**

Delete every rule for these selectors (and their `[open]`, `::backdrop`, `@starting-style` siblings):

- `.package-dialog`, `.package-dialog-card`, `.package-dialog-header`, `.package-dialog-actions`, `.package-dialog-body`, `.package-dialog-field`, `.package-dialog-input-wrap`, `.package-dialog-label`, `.package-dialog-note`, `.package-dialog-action-row`, `.package-dialog-close` — **but keep `.package-dialog-value` and `.package-dialog-value-mono`** (still used by content inside the migrated dialog)
- `.about-dialog`, `.about-icon`, `.about-title`, `.about-subtitle` — the migrated markup uses `.modal-title` / `.modal-subtitle` in their place
- `.settings-dialog`, `.settings-layout`, `.settings-nav`, `.settings-nav-title`, `.settings-nav-btn`, `.settings-content`, `.settings-section-title`, `.settings-section-desc`, `.settings-section-desc code`, `.settings-footer`, `.settings-close`

- [ ] **Step 2: Rename `.settings-section` state modifier**

The rules

```css
.settings-section { display: none; }
.settings-section.active { display: block; }
```

must remain — they are what hides inactive content panels in the nav/body layout. Rename `.settings-section.active` → `.settings-section.is-active` to match the new state convention. Keep the `.settings-section { display: none; }` rule as-is.

Leave the following content-specific rules untouched (still referenced by the migrated HTML):
- `.about-version`, `.about-desc`, `.about-tech`, `.about-tag`, `.about-links`, `.about-copy`, `.about-legal`
- `.key-row`, `.key-label`, `.key-field`, `.key-actions`, `.key-status`, `.key-status-badge` (and its `.keychain`/`.env`/`.dotenv` variants)
- `.package-dialog-value`, `.package-dialog-value-mono`

- [ ] **Step 3: Sanity gate**

```bash
rg '^\s*\.(package-dialog(-card|-header|-actions|-body|-field|-input-wrap|-label|-note|-action-row|-close)?)\b' frontend/static/styles/
rg '^\s*\.(settings-(dialog|layout|nav|nav-btn|nav-title|content|section-title|section-desc|footer|close))\b' frontend/static/styles/
rg '^\s*\.(about-dialog|about-icon|about-title|about-subtitle)\b' frontend/static/styles/
rg '\.settings-section\.active\b' frontend/static/styles/
```
Expected: zero matches for every gate. `.settings-section` alone (without `.active`) should still match exactly twice in `04-shell-layout.css` (base rule + `.is-active` variant after rename).

### Task 8.8: Commit PR #8

- [ ] **Step 1: Validate**

Run: `scripts/validate.sh`
Expected: PASS.

- [ ] **Step 2: Smoke**

Run: `cargo tauri dev`. Open every dialog (Settings via gear/⌘,, Package actions menu → Edit Name, About via menu). Confirm:
- Esc closes each dialog
- Backdrop click closes each dialog
- Clicking the header ✕ closes each dialog
- Focus returns to the trigger button after close
- "Delete Overlay" now prompts via `PickleUI.modal.confirm` with the danger-styled button

- [ ] **Step 3: Commit**

```bash
git add frontend/static/styles/components/modal.css frontend/static/app/ui/modal.js frontend/tests/ui/modal.test.js frontend/static/style.css frontend/index.html frontend/static/app/00-core.js frontend/static/app/05-settings.js frontend/static/app/06-shell.js frontend/static/styles/04-shell-layout.css
git commit -m "$(cat <<'EOF'
Lane A: modal primitive + dialog migration

- Add components/modal.css (.modal, .modal-sm/md/lg, .modal-with-nav, header/body/footer, .modal-nav)
- Add ui/modal.js (PickleUI.modal.{open,close,confirm})
- Migrate Package, About, Settings dialogs to the new primitive
- Replace window.confirm for delete-overlay with PickleUI.modal.confirm (danger tone)
- Rename .settings-section.active state modifier to .is-active
- Delete .package-dialog-*, .settings-*, .about-dialog CSS
EOF
)"
```

- [ ] **Step 4: Append logbook entry**

`Lane A PR #8: modal primitive landed. All three dialogs (Package, About, Settings) migrated. window.confirm replaced with PickleUI.modal.confirm for delete-overlay. Lane A complete.`

- [ ] **Step 5: Update `todo.md`**

Remove the "Execute Lane A design-system-unification" backlog entry (it's done). Mark Lanes C / B / D as next.

---

## Validation checklist (run before calling Lane A complete)

- [ ] `scripts/validate.sh` passes (Rust + frontend tests green, fmt + clippy clean)
- [ ] Every `rg` sanity gate listed in PR #2, #3, #5, #6, #7, #8 returns zero matches
- [ ] All three dialogs visibly use `.modal`-shaped chrome
- [ ] All four empty states visibly use `.empty-state` chrome
- [ ] The Pin/Peripheral toggle renders as a segmented group; the right-panel 5-tab strip uses underline style
- [ ] Package menu, save menu, and part picker suggestions render via `PickleUI.dropdown`
- [ ] Status bar tones animate correctly: busy → success → idle; errors turn the bar red
- [ ] Toasts appear bottom-right, auto-dismiss info/success/warn after 5 s, error + progress stay
- [ ] Tooltip displays on hover after a 300 ms delay; `[title]` attributes have been swept off
- [ ] `frontend/tests/ui/*.test.js` — every primitive helper has passing coverage
- [ ] `cargo tauri dev`: dark ↔ light theme toggle renders correctly; ⌘S / ⌘Z shortcuts still work

---

## Out of scope (for follow-on lanes)

- **Lane C:** when to toast vs. status-bar; progress indicators on long operations; diff-before-apply flow for destructive actions
- **Lane B:** keyboard navigation in dropdowns and tab strips (arrow keys, Home/End); undo/redo UI; ARIA polish beyond the basics
- **Lane D:** Pin View / Peripheral View legends, active-filter indicators, reserved-pin muting; CLC designer layout improvements
