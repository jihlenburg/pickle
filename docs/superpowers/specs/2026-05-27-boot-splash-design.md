# Boot Splash — bridge the WebView startup gap

**Status:** Approved for planning (2026-05-27)
**Author:** Collaborative design session
**Scope:** Frontend (`frontend/index.html`, `frontend/static/app/08-bootstrap.js`) + a one-line Tauri config change (`src-tauri/tauri.conf.json`). No Rust source changes, no new plugins, no new dependencies.

## 1. Purpose

When the packaged `pickle.app` launches from the macOS dock, the user currently sees a noticeable interval of empty black window between the WebView appearing and the frontend painting its first useful content. The black window persists until `frontend/index.html` has parsed, `static/style.css` has loaded, and the 30-script bootstrap chain has finished initializing `08-bootstrap.js:initializeApp()`.

This spec bridges that visual gap with a minimal inline boot splash. The window opens already showing the splash, holds the splash through the initialization chain, and fades to the live UI once the application shell is wired up.

**Success criteria:**

- No black window flash visible on cold launch (after reboot) or warm launch.
- The splash shows the existing pickle icon and product name on a theme-appropriate background.
- The splash respects the user's saved theme (dark vs. light) without waiting for `style.css` / `tokens.css` to load.
- The splash fades out and is removed once `initializeApp()` completes, on both the device-load and welcome-intro startup paths.
- Behavior is identical in `tauri dev` and the release `.app` bundle.
- A frontend JS error before `initializeApp()` cannot leave the window permanently invisible.

## 2. Design decisions

### 2.1 Shape: inline-HTML splash + Tauri `visible: false`

Two coordinated mechanisms, chosen because they decompose the perceived gap into two distinct phases that need different treatments:

1. **Tauri window starts invisible** (`tauri.conf.json` → `windows[0].visible = false`) and is shown via `__TAURI__.window.getCurrentWindow().show()` from an inline `<script>` placed inside `<body>`. The `show()` call runs inside a `requestAnimationFrame` so the splash is painted before the native window unhides — the user never sees an unpainted black window.

2. **Inline boot-splash element** (`<div id="boot-splash">` at the top of `<body>`, plus a small inline `<style>` block in `<head>`) remains visible from window-show until the application shell is ready. At the end of `initializeApp()`, `hideBootSplash()` adds an `is-hidden` class that runs a 150 ms opacity fade, then removes the element from the DOM.

Alternatives considered and rejected:

- **Inline splash only, no Tauri `visible: false`.** Simpler (no Tauri config change, no `show()` call) but leaves a brief pre-splash black flash while the WebView and HTML parser spin up. On cold launches this flash is the most noticeable part of the gap.
- **`tauri-plugin-splashscreen` / two-window setup.** Adds a plugin dependency and a second window to manage. Heavier than needed for a logo-only splash; not chosen.
- **Status text or spinner.** Considered and explicitly rejected — the chosen scope is "just a logo, never black." No IPC plumbing required.
- **Window-hide-until-ready with no splash element.** Eliminates all flashes by keeping the window invisible until `initializeApp()` returns, but introduces a "did clicking the dock icon do anything?" feel during the 1–2 s startup. Not chosen for that reason.

### 2.2 Scope: logo only, exit at shell-ready

Per the brainstorming session's narrowed scope:

- **In scope:** centered icon + product wordmark, theme-aware background, 150 ms opacity fade-out at the end of `initializeApp()`, Tauri window hidden until first splash paint.
- **Out of scope:** spinner, loading indicator, progress text, status messages, IPC-driven progress events from Rust `setup()`, a separate splash window, persisting the splash through device-load.

If a startup device is configured, `initializeApp()` does `await loadDevice(...)` before returning, so the splash already covers the device-load case for free — the splash hides only when the shell is fully ready in either path.

### 2.3 Where the splash CSS lives

The splash's CSS lives in an inline `<style>` block in `<head>`, placed **before** the `<link rel="stylesheet" href="static/style.css">`. This matters because:

- Browser render is blocked until all `<head>` stylesheets are loaded; an inline `<style>` is parsed immediately as part of the HTML.
- Putting the splash CSS inline means the splash can paint as soon as the body is parsed, without waiting for `style.css` or `tokens.css` to load.
- The splash CSS is self-contained — it does not depend on tokens, variables, or any class defined in the external stylesheets.

The cost is roughly 20 lines of CSS duplicated outside the design system. That duplication is intentional and confined to splash-specific selectors (`#boot-splash`, `#boot-splash .boot-splash-icon`, `#boot-splash .boot-splash-name`); none of it leaks into the rest of the app.

### 2.4 Theme awareness without `tokens.css`

The splash needs to render on the user's saved background color before `tokens.css` is available. Two layered fallbacks:

1. `@media (prefers-color-scheme: dark)` covers users on system-followed themes who haven't overridden the in-app theme.
2. Selectors against the attribute that `config.js`'s `applyDocumentTheme(document, defaults.themeMode)` writes onto `<html>` (verify the exact attribute name during implementation; expected `data-theme="dark"|"light"`) override the media query when the user has explicitly chosen a theme.

The inline `<script>` block in `<head>` that runs `applyDocumentTheme` already executes synchronously before body parsing, so the theme class is in place on `<html>` before the splash element renders.

## 3. Components

### 3.1 `frontend/index.html` — splash markup, inline styles, window-show

Three additions, all confined to `index.html`:

**a)** Inline `<style>` block in `<head>`, after the existing inline theme-apply `<script>` and before `<link rel="stylesheet" href="static/style.css">`. Contains only `#boot-splash` rules: fixed full-viewport position, high z-index, flex centering, light/dark background variants, 150 ms opacity transition, and an `.is-hidden { opacity: 0; pointer-events: none }` modifier.

**b)** Splash element as the first child of `<body>`:

```html
<div id="boot-splash" aria-hidden="true">
    <img src="static/pickle-icon.png" alt="" class="boot-splash-icon">
    <div class="boot-splash-name">pickle</div>
</div>
```

`aria-hidden="true"` so screen readers don't announce the decorative splash. The `<img>`'s empty `alt=""` reinforces that the wordmark below is the accessible name.

**c)** Inline `<script>` immediately after the splash element:

```html
<script>
    requestAnimationFrame(() => {
        try {
            window.__TAURI__?.window?.getCurrentWindow?.()?.show?.();
        } catch (_) { /* show() will be retried in 08-bootstrap.js */ }
    });
</script>
```

The optional-chaining and try/catch make this call safe even if `__TAURI__.window` isn't yet hydrated for any reason; `initializeApp()` will retry `show()` defensively as a second-chance path.

### 3.2 `frontend/static/app/08-bootstrap.js` — `hideBootSplash()`

A small function added near `initializeApp()`:

```js
function hideBootSplash() {
    const splash = document.getElementById('boot-splash');
    if (!splash) return;
    splash.classList.add('is-hidden');
    // Remove after the CSS opacity transition (150ms) plus a small safety margin.
    setTimeout(() => splash.remove(), 200);
}
```

`initializeApp()` currently has two exit points (an early `return` when there is no startup target, and the natural fall-through end when a device is loaded). The implementation can either (a) call `hideBootSplash()` at both exit points, or (b) refactor the function to wrap the post-`loadAppSettings` work in `try { … } finally { hideBootSplash(); }` so the call site is single. The plan will choose one; both are correct.

In addition, `initializeApp()` defensively re-invokes `__TAURI__.window.getCurrentWindow().show()` near the start so a missed inline-script `show()` is still corrected before the shell-ready transition runs.

### 3.3 `src-tauri/tauri.conf.json` — `visible: false`

The only Rust-adjacent change. Add `"visible": false` to the existing `windows[0]` entry. No `Cargo.toml` change, no `lib.rs` change, no `setup()` change, no plugin added.

## 4. Failure modes

| Failure | Symptom | Recovery |
|---|---|---|
| Inline-script `show()` errors before fire | Window stays invisible after splash paint | `initializeApp()` retries `show()` near its start. Worst case the user sees the unhide and the splash transition both happen at shell-ready time — still a window, not a hang. |
| `initializeApp()` errors before reaching `hideBootSplash()` | Splash stays visible forever, window is shown but content is hidden behind splash | The splash is a normal DOM element and is not pointer-blocking once it has `is-hidden`, but in this failure case it never gets that class. Mitigation deferred — this is an existing failure mode (init error means the app doesn't work either way). Logged for future hardening if it ever surfaces. |
| `__TAURI__.window.getCurrentWindow()` API changes in a future Tauri version | `show()` call no-ops or throws | The try/catch swallows it; user sees a hidden window forever. Mitigation: lock the Tauri version pin in `Cargo.toml` and re-test on each upgrade. |
| `prefers-color-scheme` reports the wrong value (rare in WebView) | Splash background doesn't match the saved theme; brief mis-toned splash | Acceptable for 150 ms. The `data-theme` selector overrides the media query once `applyDocumentTheme` runs in `<head>`. |

The non-recommended Rust-timer safeguard (force `show()` from Rust after N seconds) is deliberately not adopted; the two-JS-path coverage above is sufficient for the actual risk profile.

## 5. Testing

**Unit tests (Node):**

- One test in `frontend/tests/ui/` (or a new `frontend/tests/bootstrap.test.js` if the existing files don't fit) that mocks `document.getElementById('boot-splash')` and verifies `hideBootSplash()` adds the `is-hidden` class and removes the element after the timeout. Use the existing `node:test` + `vm` sandbox pattern from `tests/ui/dropdown.test.js`.

No CSS or markup unit tests — the splash is pure DOM/style with no behavior beyond `hideBootSplash()`.

**Manual smoke tests:**

- Cold launch (after reboot) of `bin/pickle.app`: no black flash; splash appears; splash fades out within ~1–2 s; app UI is interactive.
- Warm launch (immediately after closing the app): same behavior, gap visibly shorter.
- `tauri dev`: same behavior in dev.
- Theme verification: switch the app theme, quit, relaunch — splash background matches the saved theme on next launch.
- Failure-path simulation: temporarily throw at the top of `initializeApp()` and confirm the inline-script `show()` still unhides the window so the user can see the (broken) app and quit.

## 6. Out of scope

- Status text, spinner, progress bar, or any IPC-driven splash content.
- A second native window dedicated to the splash.
- Persisting the splash through the first device-render.
- Any new Tauri plugin or new `Cargo.toml` dependency.
- Changes to the welcome-intro logic — the welcome screen is unrelated and continues to handle the first-run-no-device case as today.
- Animating the icon (pulse, fade-in) — the splash appears instantly and fades out only on exit.
