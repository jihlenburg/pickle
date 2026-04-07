/**
 * Application shell state and event wiring.
 *
 * Owns cross-panel shell concerns such as tab switching, catalog freshness,
 * first-launch intro behavior, startup theme behavior, and the UI event
 * bindings that connect buttons to the workflow functions implemented elsewhere.
 */

/** @type {Set<string>} Devices available locally (no download needed) */
let cachedDevices = new Set();
/** @type {string[]} Full device catalog used by the custom part picker suggestions. */
let catalogDeviceNames = [];
/** @type {string[]} Currently visible suggestion strings in the part picker popup. */
let visiblePartSuggestions = [];
let activePartSuggestionIndex = -1;
let shellEventsBound = false;
let saveMenuBound = false;
let packageMenuBound = false;
let welcomeIntroBound = false;
const shellActionHandlers = {
    load: async () => {
        dismissWelcomeIntro({ persist: true });
        await loadDevice();
    },
    generate: () => generateCode(),
    check: () => compileCheck(),
    copy_code: () => copyCode(),
    export: () => exportCode(),
    verify: () => verifyPinout(),
    pinlist: () => exportPinList(),
    save: () => saveConfig(),
    save_as: () => saveConfigAs(),
    rename: () => renameConfig(),
    open: async () => {
        dismissWelcomeIntro({ persist: true });
        await openConfigDialog();
    },
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

function escapeWelcomeText(value) {
    return String(value ?? '')
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
}

function welcomeIntroSeen() {
    return Boolean(appSettings?.onboarding?.welcome_intro_seen);
}

async function markWelcomeIntroSeen() {
    if (appSettings?.onboarding?.welcome_intro_seen) {
        return;
    }

    if (!appSettings.onboarding) {
        appSettings.onboarding = { welcome_intro_seen: true };
    } else {
        appSettings.onboarding.welcome_intro_seen = true;
    }

    try {
        await invoke('set_welcome_intro_seen', { seen: true });
    } catch (error) {
        console.warn('Failed to persist onboarding state:', error);
    }
}

function setWelcomeIntroVisible(visible) {
    const screen = $('welcome-screen');
    if (!screen) {
        return;
    }

    screen.hidden = !visible;
    screen.setAttribute('aria-hidden', String(!visible));
    document.body.classList.toggle('welcome-active', visible);
}

function dismissWelcomeIntro(options = {}) {
    if (options.persist) {
        void markWelcomeIntroSeen();
    }
    setWelcomeIntroVisible(false);
}

function focusPartSearchFromWelcome() {
    dismissWelcomeIntro({ persist: true });
    const input = $('part-input');
    if (!input) {
        return;
    }
    input.focus();
    input.select();
    updatePartSuggestions();
}

function loadWelcomeSample(partNumber) {
    const input = $('part-input');
    if (!input || !partNumber) {
        return;
    }

    input.value = String(partNumber).trim().toUpperCase();
    dismissWelcomeIntro({ persist: true });
    updatePartSuggestions();
    void loadDevice();
}

function renderWelcomeIntro() {
    const screen = $('welcome-screen');
    const config = appConfig.ui.welcomeIntro;
    if (!screen || screen.dataset.rendered === 'true') {
        return;
    }

    const cardsHtml = (config.featureCards || []).map((card) => `
        <article class="welcome-card">
            <h3>${escapeWelcomeText(card.title)}</h3>
            <p>${escapeWelcomeText(card.body)}</p>
        </article>
    `).join('');

    const samplesHtml = (config.sampleParts || []).map((sample) => `
        <button type="button" class="welcome-sample-btn" data-part="${escapeWelcomeText(sample.part)}">
            <span class="welcome-sample-label">${escapeWelcomeText(sample.label)}</span>
            <span class="welcome-sample-note">${escapeWelcomeText(sample.note)}</span>
        </button>
    `).join('');

    screen.innerHTML = `
        <div class="welcome-shell">
            <div class="welcome-hero">
                <div class="welcome-eyebrow">${escapeWelcomeText(config.eyebrow)}</div>
                <h2 id="welcome-title">${escapeWelcomeText(config.title)}</h2>
                <p class="welcome-description">${escapeWelcomeText(config.description)}</p>
                <div class="welcome-actions">
                    <button type="button" id="welcome-primary-btn" class="welcome-primary-btn">${escapeWelcomeText(config.primaryActionLabel)}</button>
                    <button type="button" id="welcome-open-btn" class="welcome-secondary-btn">${escapeWelcomeText(config.secondaryActionLabel)}</button>
                    <button type="button" id="welcome-dismiss-btn" class="welcome-tertiary-btn">${escapeWelcomeText(config.dismissActionLabel)}</button>
                </div>
                <p class="welcome-helper">${escapeWelcomeText(config.helperText)}</p>
            </div>
            <div class="welcome-detail">
                <div class="welcome-card-grid">${cardsHtml}</div>
                <div class="welcome-samples">
                    <div class="welcome-samples-title">Quick Starts</div>
                    <div class="welcome-sample-list">${samplesHtml}</div>
                </div>
            </div>
        </div>
    `;

    screen.dataset.rendered = 'true';
}

function syncWelcomeIntroVisibility(options = {}) {
    renderWelcomeIntro();

    const allow = options.allow ?? true;
    const shouldShow = Boolean(allow && !deviceData && !welcomeIntroSeen());
    setWelcomeIntroVisible(shouldShow);
}

function wireWelcomeIntro() {
    if (welcomeIntroBound) {
        return;
    }

    renderWelcomeIntro();
    const screen = $('welcome-screen');
    if (!screen) {
        return;
    }
    welcomeIntroBound = true;

    screen.addEventListener('click', (event) => {
        const actionButton = event.target.closest('#welcome-primary-btn, #welcome-open-btn, #welcome-dismiss-btn');
        if (actionButton?.id === 'welcome-primary-btn') {
            focusPartSearchFromWelcome();
            return;
        }
        if (actionButton?.id === 'welcome-open-btn') {
            dismissWelcomeIntro({ persist: true });
            void openConfigDialog();
            return;
        }
        if (actionButton?.id === 'welcome-dismiss-btn') {
            dismissWelcomeIntro({ persist: true });
            return;
        }

        const sampleButton = event.target.closest('.welcome-sample-btn');
        if (sampleButton?.dataset.part) {
            loadWelcomeSample(sampleButton.dataset.part);
        }
    });
}

function hidePartSuggestions() {
    const input = $('part-input');
    const popup = $('part-suggestions');
    if (!input || !popup) {
        return;
    }

    popup.hidden = true;
    popup.innerHTML = '';
    visiblePartSuggestions = [];
    activePartSuggestionIndex = -1;
    input.setAttribute('aria-expanded', 'false');
    input.removeAttribute('aria-activedescendant');
}

function setActivePartSuggestion(index) {
    const input = $('part-input');
    const popup = $('part-suggestions');
    if (!input || !popup || !visiblePartSuggestions.length) {
        return;
    }

    const clampedIndex = Math.max(0, Math.min(index, visiblePartSuggestions.length - 1));
    activePartSuggestionIndex = clampedIndex;

    popup.querySelectorAll('.part-suggestion').forEach((button, buttonIndex) => {
        const isActive = buttonIndex === clampedIndex;
        button.classList.toggle('is-active', isActive);
        button.setAttribute('aria-selected', String(isActive));
        if (isActive) {
            input.setAttribute('aria-activedescendant', button.id);
            button.scrollIntoView({ block: 'nearest' });
        }
    });
}

function applyPartSuggestion(deviceName) {
    const input = $('part-input');
    if (!input || !deviceName) {
        return;
    }

    input.value = deviceName;
    hidePartSuggestions();
    input.focus();
}

function rankDeviceSuggestion(deviceName, query) {
    const matchIndex = deviceName.indexOf(query);
    if (matchIndex < 0) {
        return null;
    }

    return {
        deviceName,
        matchIndex,
        lengthDelta: deviceName.length - query.length,
    };
}

function findPartSuggestions(query) {
    const normalizedQuery = String(query || '').trim().toUpperCase();
    if (!normalizedQuery) {
        return [];
    }

    return catalogDeviceNames
        .map((deviceName) => rankDeviceSuggestion(deviceName, normalizedQuery))
        .filter(Boolean)
        .sort((left, right) => (
            left.matchIndex - right.matchIndex
            || left.lengthDelta - right.lengthDelta
            || left.deviceName.localeCompare(right.deviceName)
        ))
        .slice(0, appConfig.ui.partPicker.maxSuggestions)
        .map((entry) => entry.deviceName);
}

function renderPartSuggestions(matches) {
    const input = $('part-input');
    const popup = $('part-suggestions');
    if (!input || !popup) {
        return;
    }

    if (!matches.length) {
        hidePartSuggestions();
        return;
    }

    visiblePartSuggestions = matches;
    popup.innerHTML = '';

    matches.forEach((deviceName, index) => {
        const button = document.createElement('button');
        button.type = 'button';
        button.id = `part-suggestion-${index}`;
        button.className = 'part-suggestion';
        button.setAttribute('role', 'option');
        button.dataset.part = deviceName;

        const partLabel = document.createElement('span');
        partLabel.className = 'part-suggestion-part';
        partLabel.textContent = deviceName;
        button.appendChild(partLabel);

        if (cachedDevices.has(deviceName)) {
            const metaLabel = document.createElement('span');
            metaLabel.className = 'part-suggestion-meta';
            metaLabel.textContent = appConfig.ui.partPicker.cachedLabel;
            button.appendChild(metaLabel);
        }

        popup.appendChild(button);
    });

    popup.hidden = false;
    input.setAttribute('aria-expanded', 'true');
    setActivePartSuggestion(0);
}

function updatePartSuggestions() {
    const input = $('part-input');
    if (!input) {
        return;
    }

    const normalizedValue = input.value.toUpperCase();
    if (normalizedValue !== input.value) {
        input.value = normalizedValue;
    }

    renderPartSuggestions(findPartSuggestions(normalizedValue));
}

function handlePartPickerKeydown(event) {
    const hasSuggestions = visiblePartSuggestions.length > 0;

    if (event.key === 'ArrowDown') {
        event.preventDefault();
        if (!hasSuggestions) {
            updatePartSuggestions();
            return;
        }
        setActivePartSuggestion((activePartSuggestionIndex + 1) % visiblePartSuggestions.length);
        return;
    }

    if (event.key === 'ArrowUp') {
        event.preventDefault();
        if (!hasSuggestions) {
            updatePartSuggestions();
            return;
        }
        setActivePartSuggestion(
            (activePartSuggestionIndex - 1 + visiblePartSuggestions.length) % visiblePartSuggestions.length
        );
        return;
    }

    if (event.key === 'Escape') {
        if (hasSuggestions) {
            event.preventDefault();
            hidePartSuggestions();
        }
        return;
    }

    if (event.key === 'Tab' && hasSuggestions && activePartSuggestionIndex >= 0) {
        applyPartSuggestion(visiblePartSuggestions[activePartSuggestionIndex]);
        return;
    }

    if (event.key === 'Enter') {
        if (hasSuggestions && activePartSuggestionIndex >= 0) {
            event.preventDefault();
            applyPartSuggestion(visiblePartSuggestions[activePartSuggestionIndex]);
        }
        dismissWelcomeIntro({ persist: true });
        void loadDevice();
    }
}

function wirePartPicker() {
    const input = $('part-input');
    const popup = $('part-suggestions');
    if (!input || !popup) {
        return;
    }

    input.addEventListener('input', () => {
        updatePartSuggestions();
    });
    input.addEventListener('focus', () => {
        updatePartSuggestions();
    });
    input.addEventListener('keydown', handlePartPickerKeydown);

    popup.addEventListener('mousedown', (event) => {
        event.preventDefault();
    });
    popup.addEventListener('click', (event) => {
        const button = event.target.closest('.part-suggestion');
        if (!button?.dataset.part) {
            return;
        }
        applyPartSuggestion(button.dataset.part);
    });

    document.addEventListener('click', (event) => {
        if (!event.target.closest('.part-picker-field')) {
            hidePartSuggestions();
        }
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

function closePackageMenu() {
    const menu = $('pkg-menu');
    const button = $('pkg-menu-btn');
    if (!menu || !button) {
        return;
    }

    menu.hidden = true;
    button.setAttribute('aria-expanded', 'false');
}

function refreshPackageMenuState() {
    const group = $('pkg-control-group');
    const menu = $('pkg-menu');
    const button = $('pkg-menu-btn');
    const editButton = $('pkg-edit-name-btn');
    const resetButton = $('pkg-reset-name-btn');
    const deleteButton = $('pkg-delete-btn');
    if (!group || !menu || !button || !editButton || !resetButton || !deleteButton) {
        return;
    }

    const hasPackage = Boolean(deviceData?.selected_package);
    if (!hasPackage) {
        closePackageMenu();
        hideElement(group);
        return;
    }

    const ui = appConfig.ui.packageManager;
    button.textContent = ui.menuButtonLabel;
    button.title = ui.menuButtonTitle;
    button.setAttribute('aria-label', ui.menuButtonTitle);
    editButton.textContent = ui.menuEditLabel;
    resetButton.textContent = ui.menuResetLabel;
    deleteButton.textContent = ui.menuDeleteLabel;
    resetButton.disabled = !hasPackageDisplayNameOverride(deviceData.selected_package, selectedPackageMeta());
    deleteButton.hidden = !selectedPackageIsOverlay();
    deleteButton.disabled = !selectedPackageIsOverlay();
}

function wirePackageMenu() {
    if (packageMenuBound) {
        return;
    }

    const menu = $('pkg-menu');
    const button = $('pkg-menu-btn');
    if (!menu || !button) {
        return;
    }

    packageMenuBound = true;
    refreshPackageMenuState();

    button.addEventListener('click', (event) => {
        event.stopPropagation();
        refreshPackageMenuState();
        const nextOpen = button.getAttribute('aria-expanded') !== 'true';
        menu.hidden = !nextOpen;
        button.setAttribute('aria-expanded', String(nextOpen));
    });

    $('pkg-edit-name-btn')?.addEventListener('click', () => {
        closePackageMenu();
        showPackageManagerDialog();
    });
    $('pkg-reset-name-btn')?.addEventListener('click', () => {
        closePackageMenu();
        void resetSelectedPackageDisplayName();
    });
    $('pkg-delete-btn')?.addEventListener('click', () => {
        closePackageMenu();
        void deleteSelectedOverlayPackage();
    });

    document.addEventListener('click', (event) => {
        if (!event.target.closest('.package-split')) {
            closePackageMenu();
        }
    });
    document.addEventListener('keydown', (event) => {
        if (event.key === 'Escape') {
            closePackageMenu();
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
    wirePackageMenu();
    wirePartPicker();

    $('pkg-select')?.addEventListener('change', (event) => {
        closePackageMenu();
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
        cachedDevices = new Set(data.cached || []);
        catalogDeviceNames = Array.isArray(data.devices) ? data.devices.slice() : [];

        if (document.activeElement === $('part-input')) {
            updatePartSuggestions();
        }

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
    wirePackageManagerDialog();
    wireSettingsDialog();
    wireWelcomeIntro();
    initializeConfigDocumentUi();
    const checkButton = $('check-btn');
    if (checkButton) {
        checkButton.textContent = appConfig.ui.compiler.buttonFallbackLabel;
    }
}
