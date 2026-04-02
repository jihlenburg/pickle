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

function bindClick(id, handler) {
    const element = $(id);
    if (element) {
        element.addEventListener('click', handler);
    }
}

function wireShellEventListeners() {
    if (shellEventsBound) {
        return;
    }
    shellEventsBound = true;

    bindClick('load-btn', () => loadDevice());
    bindClick('gen-btn', generateCode);
    bindClick('check-btn', compileCheck);
    bindClick('copy-btn', copyCode);
    bindClick('export-btn', exportCode);
    bindClick('verify-btn', verifyPinout);
    bindClick('pinlist-btn', exportPinList);
    bindClick('save-btn', saveConfig);
    bindClick('load-btn-file', openConfigDialog);
    bindClick('index-badge', refreshIndex);

    $('part-input')?.addEventListener('keydown', (event) => {
        if (event.key === 'Enter') {
            loadDevice();
        }
    });

    $('pkg-select')?.addEventListener('change', (event) => {
        loadDevice(event.target.value);
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
