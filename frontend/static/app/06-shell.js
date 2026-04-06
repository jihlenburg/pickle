/**
 * Application shell state and event wiring.
 *
 * Owns cross-panel shell concerns such as tab switching, catalog freshness,
 * startup theme behavior, and the UI event bindings that connect buttons to
 * the workflow functions implemented elsewhere.
 */

/** @type {Set<string>} Devices available locally (no download needed) */
let cachedDevices = new Set();
let shellEventsBound = false;
let saveMenuBound = false;
const shellActionHandlers = {
    load: () => loadDevice(),
    generate: () => generateCode(),
    check: () => compileCheck(),
    copy_code: () => copyCode(),
    export: () => exportCode(),
    verify: () => verifyPinout(),
    pinlist: () => exportPinList(),
    save: () => saveConfig(),
    save_as: () => saveConfigAs(),
    rename: () => renameConfig(),
    open: () => openConfigDialog(),
    refresh_index: () => refreshIndex(),
    undo: () => undo(),
    redo: () => redo(),
    about: () => showAboutDialog(),
    settings: () => showSettingsDialog(),
};

function bindClick(id, handler) {
    const element = $(id);
    if (element) {
        element.addEventListener('click', handler);
    }
}

function runShellAction(action) {
    const handler = shellActionHandlers[action];
    if (!handler) {
        return false;
    }
    void handler();
    return true;
}

function bindShellAction(id, action) {
    bindClick(id, () => {
        runShellAction(action);
    });
}

function closeSaveMenu() {
    const menu = $('save-menu');
    const button = $('save-menu-btn');
    if (!menu || !button) {
        return;
    }
    menu.hidden = true;
    button.setAttribute('aria-expanded', 'false');
}

function toggleSaveMenu() {
    const menu = $('save-menu');
    const button = $('save-menu-btn');
    if (!menu || !button) {
        return;
    }
    const willOpen = menu.hidden;
    menu.hidden = !willOpen;
    button.setAttribute('aria-expanded', willOpen ? 'true' : 'false');
}

function wireSaveMenu() {
    if (saveMenuBound) {
        return;
    }
    saveMenuBound = true;

    bindClick('save-menu-btn', (event) => {
        event.stopPropagation();
        toggleSaveMenu();
    });
    bindClick('save-as-btn', () => {
        closeSaveMenu();
        runShellAction('save_as');
    });
    bindClick('rename-btn', () => {
        closeSaveMenu();
        runShellAction('rename');
    });

    document.addEventListener('click', (event) => {
        if (!event.target.closest('.save-split')) {
            closeSaveMenu();
        }
    });
    document.addEventListener('keydown', (event) => {
        if (event.key === 'Escape') {
            closeSaveMenu();
        }
    });
}

function wireShellEventListeners() {
    if (shellEventsBound) {
        return;
    }
    shellEventsBound = true;

    bindShellAction('load-btn', 'load');
    bindShellAction('gen-btn', 'generate');
    bindShellAction('check-btn', 'check');
    bindShellAction('copy-btn', 'copy_code');
    bindShellAction('export-btn', 'export');
    bindShellAction('verify-btn', 'verify');
    bindShellAction('pinlist-btn', 'pinlist');
    bindShellAction('save-btn', 'save');
    bindShellAction('load-btn-file', 'open');
    bindShellAction('index-badge', 'refresh_index');
    bindShellAction('settings-btn', 'settings');
    wireSaveMenu();

    $('part-input')?.addEventListener('keydown', (event) => {
        if (event.key === 'Enter') {
            loadDevice();
        }
    });

    $('pkg-select')?.addEventListener('change', (event) => {
        void loadDevice(event.target.value, { preserveState: true, markDirty: true });
    });

    $('code-tabs')?.addEventListener('click', (event) => {
        const button = event.target.closest('.code-tab');
        if (button?.dataset.file) {
            showTab(button.dataset.file);
        }
    });

    document.querySelectorAll('.right-tab').forEach((tab) => {
        tab.addEventListener('click', () => switchRightTab(tab.dataset.tab));
    });

    document.querySelectorAll('.view-toggle-btn').forEach((button) => {
        button.addEventListener('click', () => switchView(button.dataset.view));
    });
}

// Right-panel tab switching (Code / Verification)
function switchRightTab(tabName) {
    const targetTab = document.querySelector(`.right-tab[data-tab="${tabName}"]`);
    if (targetTab?.disabled || targetTab?.classList.contains('is-disabled')) {
        if (tabName === 'clc' && typeof setStatus === 'function') {
            setStatus('This device has no CLC peripheral.');
        }
        return;
    }

    document.querySelectorAll('.right-tab').forEach((tab) => {
        tab.classList.toggle('active', tab.dataset.tab === tabName);
    });
    document.querySelectorAll('.right-tab-content').forEach((content) => {
        content.classList.toggle('active', content.dataset.tab === tabName);
    });
}

// Left-panel view switching (Pin / Peripheral)
function switchView(viewName) {
    activeView = viewName;

    document.querySelectorAll('.view-toggle-btn').forEach((button) => {
        button.classList.toggle('active', button.dataset.view === viewName);
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

/** Render the currently active left-panel view. */
function renderActiveView() {
    if (activeView === 'peripheral') {
        renderPeripheralView();
        return;
    }
    renderDevice();
}

function populateDeviceList() {
    invoke('list_devices').then((data) => {
        const deviceList = $('device-list');
        if (!deviceList) {
            return;
        }

        deviceList.innerHTML = '';
        cachedDevices = new Set(data.cached || []);

        (data.devices || []).forEach((deviceName) => {
            const option = document.createElement('option');
            option.value = deviceName;
            deviceList.appendChild(option);
        });

        updateIndexBadge(data.total, data.cached_count);
        void refreshIndexStatus();
    }).catch((error) => {
        console.warn('Device list fetch failed:', error);
    });
}

function updateIndexBadge(total, cached) {
    const badge = $('index-badge');
    if (!badge) return;
    if (typeof total === 'number') indexCatalogState.total = total;
    if (typeof cached === 'number') indexCatalogState.cached = cached;

    if (indexCatalogState.total <= 0) {
        badge.style.display = 'none';
        return;
    }

    const catalogConfig = appConfig.ui.catalog;
    const freshness = indexCatalogState.available
        ? (indexCatalogState.isStale ? catalogConfig.labels.stale : catalogConfig.labels.fresh)
        : catalogConfig.labels.offline;
    const ageText = typeof indexCatalogState.ageHours === 'number'
        ? appConfig.format(catalogConfig.ageFormat, {
            hours: indexCatalogState.ageHours.toFixed(catalogConfig.ageFractionDigits),
        })
        : catalogConfig.ageUnknown;

    badge.textContent = appConfig.format(catalogConfig.badgeText, {
        total: indexCatalogState.total,
        cached: indexCatalogState.cached,
        freshness,
    });
    badge.title = indexCatalogState.available
        ? appConfig.format(catalogConfig.availableTitle, { freshness, ageText })
        : catalogConfig.unavailableTitle;
    badge.dataset.stale = String(indexCatalogState.isStale);
    badge.dataset.available = String(indexCatalogState.available);
    badge.style.display = '';
}

async function refreshIndexStatus() {
    try {
        const data = await invoke('index_status');
        indexCatalogState.available = !!data.available;
        indexCatalogState.ageHours = data.age_hours;
        indexCatalogState.isStale = !!data.is_stale;
    } catch {
        indexCatalogState.available = false;
        indexCatalogState.ageHours = null;
        indexCatalogState.isStale = true;
    }

    updateIndexBadge();
}

async function refreshIndex() {
    const badge = $('index-badge');
    const catalogConfig = appConfig.ui.catalog;

    if (badge) {
        badge.textContent = catalogConfig.refreshingBadge;
    }
    setStatus(catalogConfig.refreshingStatus);

    try {
        const data = await invoke('refresh_index');
        if (!data.success) {
            if (badge) {
                badge.textContent = catalogConfig.refreshFailedBadge;
            }
            setStatus(catalogConfig.refreshFailedStatus);
            return;
        }

        indexCatalogState.available = true;
        indexCatalogState.ageHours = data.age_hours;
        indexCatalogState.isStale = false;
        populateDeviceList();
        setStatus(appConfig.format(catalogConfig.refreshedStatus, {
            deviceCount: data.device_count,
            packCount: data.pack_count,
        }));
    } catch {
        if (badge) {
            badge.textContent = catalogConfig.refreshFailedBadge;
        }
        setStatus(catalogConfig.refreshFailedStatus);
    }
}

/** Resolve the effective theme ('dark' or 'light') for a given mode. */
function resolveTheme(mode) {
    if (mode === 'system') {
        return window.matchMedia(appConfig.theme.mediaQuery).matches ? 'light' : 'dark';
    }
    return mode;
}

/** Label for the theme toggle button. */
function themeLabel(mode) {
    return appConfig.theme.labels[mode] || appConfig.theme.labels.system;
}

/** Initialize theme from the shared settings file and wire toggle button. */
function setupTheme() {
    const button = $('theme-toggle');
    if (!button) {
        return;
    }

    const mediaQuery = window.matchMedia(appConfig.theme.mediaQuery);
    const cycle = appConfig.theme.cycle;
    let current = normalizeThemeMode(appSettings.appearance.theme);

    const applyThemeMode = (mode) => {
        appConfig.applyDocumentTheme(document, resolveTheme(mode));
        button.textContent = themeLabel(mode);
    };

    applyThemeMode(current);

    button.addEventListener('click', async () => {
        current = cycle[current] || appConfig.defaults.themeMode;
        applyThemeMode(current);
        try {
            await saveThemeMode(current);
        } catch (error) {
            console.warn('Failed to save theme mode:', error);
        }
    });

    // When in system mode, follow OS changes in real time.
    mediaQuery.addEventListener('change', () => {
        if (current === 'system') {
            applyThemeMode('system');
        }
    });
}

// --- About dialog ---

function showAboutDialog() {
    const dialog = $('about-dialog');
    if (!dialog || dialog.open) return;

    // Populate version lazily on first open
    const versionEl = $('about-version');
    if (versionEl && !versionEl.textContent) {
        const tauriApp = window.__TAURI__?.app;
        if (tauriApp?.getVersion) {
            tauriApp.getVersion().then((v) => {
                versionEl.textContent = `Version ${v}`;
            }).catch(() => {
                versionEl.textContent = 'Version —';
            });
        }
    }

    dialog.showModal();
}

function wireAboutDialog() {
    const dialog = $('about-dialog');
    if (!dialog) return;
    // Clicks on ::backdrop register on the <dialog> element; close when
    // the click lands outside the dialog's bounding box.
    dialog.addEventListener('click', (e) => {
        const rect = dialog.getBoundingClientRect();
        if (e.clientX < rect.left || e.clientX > rect.right ||
            e.clientY < rect.top  || e.clientY > rect.bottom) {
            dialog.close();
        }
    });
    bindClick('about-close-btn', () => dialog.close());
    bindClick('about-github-btn', () => {
        const opener = window.__TAURI__?.opener;
        if (opener?.openUrl) {
            opener.openUrl('https://github.com/jihlenburg/pickle');
        }
    });
    // Escape is handled natively by <dialog>
}

function initializeShellChrome() {
    wireShellEventListeners();
    wireAboutDialog();
    wireSettingsDialog();
    initializeConfigDocumentUi();
    const checkButton = $('check-btn');
    if (checkButton) {
        checkButton.textContent = appConfig.ui.compiler.buttonFallbackLabel;
    }
}
