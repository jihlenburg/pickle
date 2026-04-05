/**
 * Final application bootstrap.
 *
 * Keeps startup orchestration thin by delegating shell behavior to
 * `06-shell.js` and datasheet verification to `07-verification.js`.
 */

let menuEventsBound = false;
let tooltipEventsBound = false;

function wireMenuActionListener() {
    if (menuEventsBound || !window.__TAURI__?.event?.listen) {
        return;
    }
    menuEventsBound = true;

    window.__TAURI__.event.listen('menu-action', (event) => {
        runShellAction(event.payload);
    });
}

function wireTooltipSystem() {
    if (tooltipEventsBound) {
        return;
    }
    tooltipEventsBound = true;

    const tooltipElement = document.createElement('div');
    tooltipElement.className = 'app-tooltip';
    document.body.appendChild(tooltipElement);

    let tooltipTimer = null;

    document.addEventListener('mouseover', (event) => {
        const target = event.target.closest('[data-tip]');
        if (!target || !target.dataset.tip) return;

        clearTimeout(tooltipTimer);
        tooltipTimer = setTimeout(() => {
            tooltipElement.textContent = target.dataset.tip;
            tooltipElement.classList.add('visible');

            const rect = target.getBoundingClientRect();
            let top = rect.top - tooltipElement.offsetHeight - 4;
            let left = rect.left;

            if (top < 4) top = rect.bottom + 4;
            const maxLeft = window.innerWidth - tooltipElement.offsetWidth - 4;
            if (left > maxLeft) left = maxLeft;
            if (left < 4) left = 4;

            tooltipElement.style.top = `${top}px`;
            tooltipElement.style.left = `${left}px`;
        }, appConfig.ui.timings.tooltipDelayMs);
    });

    document.addEventListener('mouseout', (event) => {
        const target = event.target.closest('[data-tip]');
        if (!target) return;
        clearTimeout(tooltipTimer);
        tooltipElement.classList.remove('visible');
    });
}

// Initialize UI and load the configured startup device if one is available.
async function initializeApp() {
    initializeShellChrome();
    wireMenuActionListener();
    wireTooltipSystem();

    await loadAppSettings();
    setupTheme();
    void checkApiKey();
    setupOscUI();
    setupFuseUI();
    populateDeviceList();

    const startupTarget = resolveStartupTarget(appSettings);
    if (!startupTarget) {
        return;
    }

    $('part-input').value = startupTarget.partNumber;
    await loadDevice(startupTarget.package || undefined, { preserveState: false });
}

initializeApp();
