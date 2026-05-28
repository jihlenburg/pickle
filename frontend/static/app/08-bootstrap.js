/**
 * Final application bootstrap.
 *
 * Keeps startup orchestration thin by delegating shell behavior, startup intro
 * visibility, and datasheet verification to the dedicated modules that own
 * those flows.
 */

/**
 * Splash visibility state. splashVisibleSince is set when the native window
 * is shown for the first time (window.onload + rAF, or as a defensive
 * fallback from initializeApp's finally). hideBootSplash uses it to enforce
 * a minimum 800ms display so the splash reads as a deliberate brand moment
 * rather than a flash.
 */
let splashVisibleSince = null;
let windowShown = false;
const BOOT_SPLASH_MIN_DISPLAY_MS = 800;

function showWindowOnce() {
    if (windowShown) return;
    windowShown = true;
    try {
        window.__TAURI__?.window?.getCurrentWindow?.()?.show?.();
    } catch (_) { /* swallowed; we'll have to live with an invisible window */ }
    if (splashVisibleSince == null) {
        splashVisibleSince = (typeof performance !== 'undefined' && performance.now)
            ? performance.now()
            : Date.now();
    }
}

// Primary show path: wait for window.onload (all resources loaded → first
// paint guaranteed), then one rAF, then unhide the native window. This
// eliminates the show-vs-paint race that produced the brief black flash
// in the previous implementation. initializeApp's finally re-invokes
// showWindowOnce defensively in case onload + rAF never fire.
if (typeof window !== 'undefined' && typeof window.addEventListener === 'function') {
    window.addEventListener('load', () => {
        requestAnimationFrame(showWindowOnce);
    });
}

/**
 * Hide and remove the inline boot-splash element. Enforces a minimum
 * BOOT_SPLASH_MIN_DISPLAY_MS visible duration since the window was shown:
 * if the shell becomes ready before that window has elapsed, the fade is
 * delayed so the splash never reads as a flash. The 200ms post-fade
 * timeout matches the 150ms opacity transition declared in index.html's
 * inline <style> plus a small safety margin for slow paints. Safe to call
 * when no splash is in the DOM (e.g. unit tests).
 */
function hideBootSplash() {
    const splash = document.getElementById('boot-splash');
    if (!splash) return;
    const now = (typeof performance !== 'undefined' && performance.now)
        ? performance.now()
        : Date.now();
    const elapsed = splashVisibleSince != null
        ? now - splashVisibleSince
        : BOOT_SPLASH_MIN_DISPLAY_MS;
    const holdMs = Math.max(0, BOOT_SPLASH_MIN_DISPLAY_MS - elapsed);
    setTimeout(() => {
        splash.classList.add('is-hidden');
        setTimeout(() => splash.remove(), 200);
    }, holdMs);
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
        // Defensive: if window.onload + rAF never reached showWindowOnce
        // (e.g. a hung resource that starves the rAF callback), show the
        // window now so the user isn't left looking at an invisible app
        // while the splash fades.
        showWindowOnce();
        hideBootSplash();
    }
}

// Auto-invoke only when the shell dependencies (06-shell.js, etc.) are loaded.
// Lets the file be evaluated in isolation by unit tests without triggering
// initializeApp's full dependency chain.
if (typeof initializeShellChrome === 'function') {
    initializeApp();
}
