# Boot Splash Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the visible black-window flash between WebView spawn and frontend shell-ready by introducing an inline boot-splash element plus a Tauri `visible: false` + show-on-first-paint pattern.

**Architecture:** Two coordinated mechanisms. (1) Inline `<style>` + `<div id="boot-splash">` baked into `frontend/index.html`'s `<head>`/`<body>` so the splash paints as soon as the body is parsed, with theme awareness via the CSS variables that `config.js`'s `applyDocumentTheme()` already sets on `<html>` before body parsing. (2) `tauri.conf.json` sets `windows[0].visible = false`; an inline `<script>` placed immediately after the splash element calls `__TAURI__.window.getCurrentWindow().show()` inside `requestAnimationFrame`, so the native window only unhides after the splash is painted. `08-bootstrap.js:initializeApp()` runs `hideBootSplash()` at shell-ready, which adds an `is-hidden` class (150 ms opacity fade) and removes the splash element after the transition.

**Tech Stack:** HTML / CSS / vanilla JavaScript (no build step on the frontend), Tauri 2 (`__TAURI__.window.getCurrentWindow().show()` via `withGlobalTauri: true`), Node `node:test` + `vm` sandbox for unit tests.

**Spec:** `docs/superpowers/specs/2026-05-27-boot-splash-design.md`

---

## File Structure

**Created:**

- `frontend/tests/boot-splash.test.js` — Node test for `hideBootSplash()` (DOM removal after class-add + timeout). Mirrors the `vm` sandbox pattern used by `frontend/tests/ui/dropdown.test.js`.

**Modified:**

- `frontend/index.html` — adds inline `<style>` block for `#boot-splash` in `<head>` (before `<link rel="stylesheet" href="static/style.css">`); adds `<div id="boot-splash">` as first child of `<body>`; adds inline `<script>` immediately after the splash element to call `show()` inside `requestAnimationFrame`.
- `frontend/static/app/08-bootstrap.js` — adds `hideBootSplash()` function; refactors `initializeApp()` to wrap the post-`loadAppSettings` work in `try { … } finally { hideBootSplash(); }` so the splash hides on both the device-load and welcome-intro paths from a single call site; adds a defensive `show()` retry at the start of `initializeApp()`.
- `src-tauri/tauri.conf.json` — adds `"visible": false` to the existing `windows[0]` entry.

**No changes required:**

- `src-tauri/Cargo.toml`, `src-tauri/src/*.rs` — no new dependencies, no Rust source modifications.
- Any other frontend module — the splash is self-contained in `index.html` + `08-bootstrap.js`.

---

## Task 1: `hideBootSplash()` — test + implementation in `08-bootstrap.js`

This task adds the hide function and its test, but does NOT yet wire it into `initializeApp()` or add the splash markup. The function works on any element with `id="boot-splash"` and tolerates the element being absent. After this task, the test passes; the application's runtime behavior is unchanged (no splash to hide yet).

**Files:**

- Create: `frontend/tests/boot-splash.test.js`
- Modify: `frontend/static/app/08-bootstrap.js` (top of file, before `initializeApp`)

- [ ] **Step 1: Write the failing test**

Create `frontend/tests/boot-splash.test.js`:

```javascript
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function loadBootstrap() {
    return fs.readFileSync(
        path.join(__dirname, '..', 'static', 'app', '08-bootstrap.js'),
        'utf8',
    );
}

function mkDoc() {
    const removed = { value: false };
    const splash = {
        id: 'boot-splash',
        classes: new Set(),
        classList: {
            add(c) { splash.classes.add(c); },
            contains(c) { return splash.classes.has(c); },
        },
        remove() { removed.value = true; },
    };
    return {
        document: {
            getElementById(id) { return id === 'boot-splash' ? splash : null; },
        },
        splash,
        removed,
    };
}

test('hideBootSplash adds is-hidden class and removes the element after the transition', () => {
    const { document, splash, removed } = mkDoc();
    const sandbox = {
        window: {},
        document,
        setTimeout: (fn, ms) => { sandbox._lastTimeout = { fn, ms }; return 1; },
        clearTimeout: () => {},
    };
    vm.createContext(sandbox);
    vm.runInContext(loadBootstrap(), sandbox);

    sandbox.hideBootSplash();
    assert.equal(splash.classList.contains('is-hidden'), true,
        'is-hidden class added before removal');
    assert.equal(removed.value, false, 'element not removed synchronously');
    assert.equal(sandbox._lastTimeout.ms, 200,
        'removal scheduled at 200ms (150ms fade + 50ms safety margin)');

    sandbox._lastTimeout.fn();
    assert.equal(removed.value, true, 'element removed after timeout fires');
});

test('hideBootSplash is a no-op when the splash element is absent', () => {
    const sandbox = {
        window: {},
        document: { getElementById: () => null },
        setTimeout: () => 1,
        clearTimeout: () => {},
    };
    vm.createContext(sandbox);
    vm.runInContext(loadBootstrap(), sandbox);

    assert.doesNotThrow(() => sandbox.hideBootSplash());
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --test frontend/tests/boot-splash.test.js`

Expected: FAIL with `TypeError: sandbox.hideBootSplash is not a function` (or similar — the function is not yet defined in `08-bootstrap.js`).

- [ ] **Step 3: Implement `hideBootSplash()` in `08-bootstrap.js`**

Open `frontend/static/app/08-bootstrap.js` and add the function near the top of the file, before `initializeApp`. The full file currently reads:

```javascript
/**
 * Final application bootstrap.
 *
 * Keeps startup orchestration thin by delegating shell behavior, startup intro
 * visibility, and datasheet verification to the dedicated modules that own
 * those flows.
 */

let menuEventsBound = false;

function wireMenuActionListener() {
    if (menuEventsBound || !window.__TAURI__?.event?.listen) {
        return;
    }
    menuEventsBound = true;

    window.__TAURI__.event.listen('menu-action', (event) => {
        runShellAction(event.payload);
    });
}

// Initialize UI and load the configured startup device if one is available.
async function initializeApp() {
    initializeShellChrome();
    wireMenuActionListener();
    window.PickleUI.tooltip.install();

    await loadAppSettings();
    setupTheme();
    void checkApiKey();
    setupOscUI();
    setupFuseUI();
    populateDeviceList();

    const startupTarget = resolveStartupTarget(appSettings);
    if (!startupTarget) {
        syncWelcomeIntroVisibility({ allow: true });
        return;
    }

    $('part-input').value = startupTarget.partNumber;
    await loadDevice(startupTarget.package || undefined, { preserveState: false });
    syncWelcomeIntroVisibility({ allow: !deviceData });
}

initializeApp();
```

Add the `hideBootSplash` function immediately after the file's leading docstring and before `let menuEventsBound = false;`:

```javascript
/**
 * Hide and remove the inline boot-splash element. Safe to call when no
 * splash is in the DOM (e.g. unit tests, or a future refactor that drops the
 * markup). The 200ms timeout matches the 150ms opacity transition declared
 * in index.html's inline <style> plus a small safety margin for slow paints.
 */
function hideBootSplash() {
    const splash = document.getElementById('boot-splash');
    if (!splash) return;
    splash.classList.add('is-hidden');
    setTimeout(() => splash.remove(), 200);
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `node --test frontend/tests/boot-splash.test.js`

Expected: PASS — 2 tests, 0 failures.

- [ ] **Step 5: Run the full frontend test suite to confirm no regressions**

Run: `node --test frontend/tests/*.test.js frontend/tests/ui/*.test.js`

Expected: 109/109 pass (was 107/107 before this task; +2 boot-splash tests).

- [ ] **Step 6: Commit**

```bash
git add frontend/tests/boot-splash.test.js frontend/static/app/08-bootstrap.js
git commit -m "$(cat <<'EOF'
Boot splash: add hideBootSplash() + node test

Introduces hideBootSplash() in 08-bootstrap.js: adds the
is-hidden class to #boot-splash and removes the element after
200ms (matches the 150ms CSS opacity transition that lands in the
next commit). Safe to call when the splash element is absent.

Node test covers both the present-element path (class added then
element removed after timeout) and the absent-element path
(no-op, no throw). Function is not yet called from initializeApp
or referenced by any markup; that wiring lands in Task 2.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Splash markup, CSS, and `initializeApp()` wiring

Adds the visible splash element, its inline CSS, and wires `hideBootSplash()` into `initializeApp()` via a `try { … } finally { … }` block so the splash hides at shell-ready on both startup paths. After this task, the splash appears on launch and disappears at shell-ready in `tauri dev`. The pre-splash black flash (WebView spin-up) is still present; Task 3 eliminates that.

**Files:**

- Modify: `frontend/index.html` (head: add inline `<style>` block; body: add splash element as first child)
- Modify: `frontend/static/app/08-bootstrap.js` (refactor `initializeApp()` for try/finally)

- [ ] **Step 1: Add the inline `<style>` block in `<head>` of `frontend/index.html`**

The existing `<head>` reads:

```html
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>pickle — Pin Configurator</title>
    <script src="static/app/config.js"></script>
    <script>
        window.PickleConfig.applyDocumentTheme(document, window.PickleConfig.defaults.themeMode);
    </script>
    <link rel="stylesheet" href="static/style.css">
</head>
```

Insert a `<style>` block BETWEEN the inline theme-apply `<script>` and the `<link rel="stylesheet">`. The exact replacement: change the lines

```html
    </script>
    <link rel="stylesheet" href="static/style.css">
```

to

```html
    </script>
    <style>
        /* Light-mode fallback in the worst-case "JS hasn't yet set CSS vars
           on <html>" race. The var() declarations below override this once
           applyDocumentTheme() has run. */
        #boot-splash {
            position: fixed;
            inset: 0;
            z-index: 99999;
            display: flex;
            flex-direction: column;
            align-items: center;
            justify-content: center;
            gap: 16px;
            background: #ffffff;
            color: #1a1a1a;
            opacity: 1;
            transition: opacity 150ms ease-out;
        }
        @media (prefers-color-scheme: dark) {
            #boot-splash { background: #0b1220; color: #ebf2ff; }
        }
        /* Live theme wins via the inline CSS variables that config.js's
           applyDocumentTheme() writes onto <html> before body parsing. */
        #boot-splash {
            background: var(--bg, #ffffff);
            color: var(--text, #1a1a1a);
        }
        #boot-splash.is-hidden {
            opacity: 0;
            pointer-events: none;
        }
        #boot-splash .boot-splash-icon {
            width: 96px;
            height: 96px;
            object-fit: contain;
        }
        #boot-splash .boot-splash-name {
            font-family: var(--font-body, "Avenir Next", "IBM Plex Sans", "Segoe UI", sans-serif);
            font-size: 28px;
            font-weight: 500;
            letter-spacing: 0.5px;
        }
    </style>
    <link rel="stylesheet" href="static/style.css">
```

Notes for the implementer:

- The CSS variables `--bg`, `--text`, and `--font-body` are set as inline style on `<html>` by `config.js`'s `applyDocumentTheme()` (verified in `frontend/static/app/config.js:328-338`). They are available before this `<style>` resolves because the theme-apply `<script>` runs synchronously in `<head>` before the body parses.
- The hard-coded fallbacks (`#0b1220`, `#ebf2ff`) match the dark theme defaults so the splash renders correctly even in the worst-case race where the inline `<script>` has not yet set the variables.
- No external CSS file is required for the splash to render — that is the whole point of inlining.

- [ ] **Step 2: Add the splash element as the first child of `<body>`**

The existing `<body>` opens with:

```html
<body>
    <header class="app-header">
```

Change to:

```html
<body>
    <div id="boot-splash" aria-hidden="true">
        <img src="static/pickle-icon.png" alt="" class="boot-splash-icon">
        <div class="boot-splash-name">pickle</div>
    </div>

    <header class="app-header">
```

Notes:

- `aria-hidden="true"` keeps the decorative splash off the accessibility tree.
- The image's empty `alt=""` is intentional: the wordmark below provides the accessible name.
- `static/pickle-icon.png` is the file already used by `<img class="brand-icon">` in the existing header; no new asset is added.

- [ ] **Step 3: Refactor `initializeApp()` to call `hideBootSplash()` from a single exit point**

In `frontend/static/app/08-bootstrap.js`, replace the body of `initializeApp()` so the post-`loadAppSettings` work runs inside a `try { … } finally { hideBootSplash(); }` block. The current function:

```javascript
async function initializeApp() {
    initializeShellChrome();
    wireMenuActionListener();
    window.PickleUI.tooltip.install();

    await loadAppSettings();
    setupTheme();
    void checkApiKey();
    setupOscUI();
    setupFuseUI();
    populateDeviceList();

    const startupTarget = resolveStartupTarget(appSettings);
    if (!startupTarget) {
        syncWelcomeIntroVisibility({ allow: true });
        return;
    }

    $('part-input').value = startupTarget.partNumber;
    await loadDevice(startupTarget.package || undefined, { preserveState: false });
    syncWelcomeIntroVisibility({ allow: !deviceData });
}
```

Replace with:

```javascript
async function initializeApp() {
    initializeShellChrome();
    wireMenuActionListener();
    window.PickleUI.tooltip.install();

    try {
        await loadAppSettings();
        setupTheme();
        void checkApiKey();
        setupOscUI();
        setupFuseUI();
        populateDeviceList();

        const startupTarget = resolveStartupTarget(appSettings);
        if (!startupTarget) {
            syncWelcomeIntroVisibility({ allow: true });
            return;
        }

        $('part-input').value = startupTarget.partNumber;
        await loadDevice(startupTarget.package || undefined, { preserveState: false });
        syncWelcomeIntroVisibility({ allow: !deviceData });
    } finally {
        hideBootSplash();
    }
}
```

Notes:

- `finally` runs on both the early `return` (no startup target) and the natural fall-through end (device loaded), and also if a `throw` escapes one of the awaited calls — exactly the "shell-ready or shell-failed" semantics the spec requires.
- The pre-`try` work (`initializeShellChrome`, `wireMenuActionListener`, `tooltip.install`) is outside the `try` because it must finish before anything else runs and is itself synchronous and very fast; if it throws, the splash staying visible is correct (the app is unusable anyway).

- [ ] **Step 4: Re-run the full frontend test suite**

Run: `node --test frontend/tests/*.test.js frontend/tests/ui/*.test.js`

Expected: 109/109 pass (no regressions; the boot-splash tests from Task 1 still pass, and existing tests are unaffected because the splash is HTML-only with no behavioral side-effects in the test harness).

- [ ] **Step 5: Manual smoke test in `tauri dev`**

Run: `cd src-tauri && cargo tauri dev` (in a separate shell, since the command is long-running).

Expected: on app launch, you see the centered pickle icon + "pickle" wordmark on the dark (or light, depending on saved theme) background, which fades out over ~150 ms once the shell is ready. The pre-splash black flash is still present (eliminated in Task 3).

Stop the dev server (Ctrl-C) before proceeding.

- [ ] **Step 6: Commit**

```bash
git add frontend/index.html frontend/static/app/08-bootstrap.js
git commit -m "$(cat <<'EOF'
Boot splash: add inline splash markup + CSS, wire hide on shell-ready

Adds an inline <style> block in <head> (before style.css link) and a
<div id="boot-splash"> as the first child of <body>, so the splash paints
the moment the body is parsed — without waiting for tokens.css/style.css.
The splash uses var(--bg, #0b1220) / var(--text, #ebf2ff) / var(--font-body, …)
so it inherits the user's theme through the inline CSS variables that
config.js's applyDocumentTheme() already sets on <html> before body parsing,
with hard-coded fallbacks that match the dark theme defaults.

initializeApp() is refactored so the post-loadAppSettings work runs inside
try { … } finally { hideBootSplash(); }. finally fires on both startup
paths (no startup target → welcome screen; startup target → device load)
and also if an awaited call throws — so the splash hides at shell-ready
or shell-failed alike, from a single call site.

The pre-splash black flash (WebView spin-up before the body is parsed) is
still visible; the next commit eliminates that via Tauri visible=false +
show-on-first-paint.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Tauri `visible: false` + show-on-first-paint

Eliminates the pre-splash black flash by starting the native window hidden and showing it from JavaScript only after the splash has been painted (via `requestAnimationFrame`). Adds a defensive second `show()` call in `initializeApp()` so a missed inline-script `show()` is still corrected.

**Files:**

- Modify: `src-tauri/tauri.conf.json`
- Modify: `frontend/index.html` (add inline `<script>` immediately after the splash element)
- Modify: `frontend/static/app/08-bootstrap.js` (defensive `show()` retry at top of `initializeApp`)

- [ ] **Step 1: Set `visible: false` on the main window in `tauri.conf.json`**

The existing `windows` entry:

```json
"windows": [
    {
        "title": "pickle — Pin Configurator",
        "width": 1400,
        "height": 900,
        "minWidth": 1000,
        "minHeight": 600
    }
]
```

Change to:

```json
"windows": [
    {
        "title": "pickle — Pin Configurator",
        "width": 1400,
        "height": 900,
        "minWidth": 1000,
        "minHeight": 600,
        "visible": false
    }
]
```

- [ ] **Step 2: Add inline `<script>` after the splash element in `frontend/index.html`**

The splash element added in Task 2 currently reads:

```html
<body>
    <div id="boot-splash" aria-hidden="true">
        <img src="static/pickle-icon.png" alt="" class="boot-splash-icon">
        <div class="boot-splash-name">pickle</div>
    </div>

    <header class="app-header">
```

Add a `<script>` block immediately after the splash element (and before `<header>`):

```html
<body>
    <div id="boot-splash" aria-hidden="true">
        <img src="static/pickle-icon.png" alt="" class="boot-splash-icon">
        <div class="boot-splash-name">pickle</div>
    </div>
    <script>
        // Show the native window once the splash has had a chance to paint.
        // tauri.conf.json starts the window with visible: false to avoid the
        // pre-splash black flash; requestAnimationFrame ensures the splash
        // element is in the layout tree before show() runs.
        requestAnimationFrame(() => {
            try {
                window.__TAURI__?.window?.getCurrentWindow?.()?.show?.();
            } catch (_) {
                // 08-bootstrap.js retries show() defensively at the top of
                // initializeApp() so a failure here is recoverable.
            }
        });
    </script>

    <header class="app-header">
```

Notes:

- `requestAnimationFrame` runs the callback just before the next paint, so when `show()` runs the splash is in the layout tree and ready to render — the unhide reveals an already-painted splash.
- Optional chaining + try/catch tolerates the `__TAURI__.window` API being unavailable (e.g. when serving `index.html` outside Tauri for tests) and tolerates any future API renames silently — at the cost of a hidden window if the retry in Step 3 also fails.

- [ ] **Step 3: Add defensive `show()` retry at the top of `initializeApp`**

In `frontend/static/app/08-bootstrap.js`, the current `initializeApp()` (as left by Task 2) starts with:

```javascript
async function initializeApp() {
    initializeShellChrome();
    wireMenuActionListener();
    window.PickleUI.tooltip.install();

    try {
        await loadAppSettings();
```

Insert a defensive `show()` call as the very first statement of the function (before `initializeShellChrome()`):

```javascript
async function initializeApp() {
    // Defensive retry: the inline <script> after #boot-splash already calls
    // show() inside requestAnimationFrame. This second call covers the rare
    // case where the inline call failed or never ran, so the user can never
    // be left looking at a permanently invisible window.
    try {
        window.__TAURI__?.window?.getCurrentWindow?.()?.show?.();
    } catch (_) { /* swallowed; the splash → shell-ready fade will still run */ }

    initializeShellChrome();
    wireMenuActionListener();
    window.PickleUI.tooltip.install();

    try {
        await loadAppSettings();
```

- [ ] **Step 4: Re-run the full frontend test suite**

Run: `node --test frontend/tests/*.test.js frontend/tests/ui/*.test.js`

Expected: 109/109 pass. The boot-splash tests sandbox `window` and `document` without a `__TAURI__` namespace, and optional chaining gracefully skips the `show()` call — no test changes are needed.

- [ ] **Step 5: Manual smoke test in `tauri dev`**

Run: `cd src-tauri && cargo tauri dev`

Expected: app launches with NO black flash; the window simply appears already showing the splash, which then fades to the live shell. The splash is visible for a noticeably shorter perceived duration than after Task 2 because the user doesn't see the WebView-spin-up gap.

Stop the dev server (Ctrl-C) before proceeding.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/tauri.conf.json frontend/index.html frontend/static/app/08-bootstrap.js
git commit -m "$(cat <<'EOF'
Boot splash: hide Tauri window until the splash paints

Sets windows[0].visible=false in tauri.conf.json and calls
__TAURI__.window.getCurrentWindow().show() from an inline <script>
placed immediately after the splash element in index.html. The
show() call runs inside requestAnimationFrame so the splash is
painted before the native window unhides — the user sees the
splash on the first frame they see anything at all, instead of a
brief black WebView background.

initializeApp() also retries show() at its first statement as a
safety net. If the inline-script show() failed (e.g. __TAURI__
not yet hydrated for any reason), the retry runs ~100ms later and
recovers, so a frontend JS error can never leave the window
permanently invisible.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Rebuild release bundle + manual verification

Rebuild `bin/pickle.app` with the splash and confirm the user-visible behavior on a real (non-`tauri dev`) launch.

**Files:** None modified directly; only build outputs change.

- [ ] **Step 1: Run the project's release-bundle script**

Run: `bash scripts/release-app.sh`

Expected: completes in ~30 s (warm `target/` cache). Output ends with:

```
Copied latest app bundle to:
  /Users/jihlenburg/pickle/bin/pickle.app
```

- [ ] **Step 2: Launch the rebuilt `pickle.app` from a cold-ish state**

Quit any running pickle instance, then double-click `bin/pickle.app` from Finder (or `open bin/pickle.app` from a terminal).

Verify in this order:

1. From clicking through to the window appearing: no visible black-window flash. The window appears already showing the centered pickle icon + "pickle" wordmark.
2. The splash background matches the saved theme (light or dark — open Settings → Appearance to confirm before testing).
3. After ~0.5–2 s, the splash fades over ~150 ms and the live shell (header, part picker, panels) appears.
4. No "ghost" splash residue: the icon and wordmark are gone, not just transparent. Pointer events on the shell work immediately.
5. Quit and relaunch (warm launch): the same behavior, with a perceptibly shorter splash duration.

- [ ] **Step 3: Verify the failure-mode guard works**

In `frontend/static/app/08-bootstrap.js`, temporarily inject `throw new Error('test')` as the FIRST line inside the `try` block of `initializeApp` (i.e. immediately before `await loadAppSettings()`). Rebuild (`bash scripts/release-app.sh`) and launch.

Expected: the window still becomes visible (the inline-script `show()` ran) and the splash stays visible (because `finally` runs `hideBootSplash()` even on throw — confirm the splash DOES fade out, and the (broken) shell is visible underneath; if `finally` didn't run, the splash would linger).

Then revert the test throw, rebuild, and confirm normal operation again.

- [ ] **Step 4: Update `logbook.md` with the verification result**

Add a bullet to today's logbook entry (top of `logbook.md` under the most recent `## YYYY-MM-DD` heading) summarizing what was verified on the rebuilt bundle: cold launch, warm launch, theme matching, fade-out cleanliness, failure-mode guard.

- [ ] **Step 5: Commit `logbook.md`** (only if it was actually edited in Step 4)

```bash
git add logbook.md
git commit -m "logbook: boot-splash verification on the rebuilt bundle"
```

---

## Out of scope (deferred per the spec)

- No spinner, no progress text, no IPC plumbing.
- No new Tauri plugin or new `Cargo.toml` dependency.
- No animated icon (pulse, fade-in) — the splash appears instantly and fades only on exit.
- No persistence of splash through the first device-render after shell-ready.
- No CSS / markup test for the splash element itself — its behavior is purely declarative.
