/**
 * Final application bootstrap.
 *
 * Keeps startup orchestration thin by delegating shell behavior, startup intro
 * visibility, and datasheet verification to the dedicated modules that own
 * those flows.
 */

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

// Auto-invoke only when the shell dependencies (06-shell.js, etc.) are loaded.
// Lets the file be evaluated in isolation by unit tests without triggering
// initializeApp's full dependency chain.
if (typeof initializeShellChrome === 'function') {
    initializeApp();
}
