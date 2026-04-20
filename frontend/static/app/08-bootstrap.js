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
