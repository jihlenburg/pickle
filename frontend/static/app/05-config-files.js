/**
 * Configuration document lifecycle and import/export flows.
 *
 * Owns the persisted config payload, dirty tracking, save/load workflows, and
 * the small shell affordances that reflect document state in the header.
 */

/** @type {{path:?string, savedContents:?string}} */
let configDocument = {
    path: null,
    savedContents: null,
};
let lastWindowTitle = '';

// =============================================================================
// Config document state
// =============================================================================

function configFileBasename(path) {
    return path ? String(path).split(/[\\/]/).pop() : appConfig.ui.configFiles.unsavedName;
}

function buildCurrentConfigPayload() {
    if (!deviceData) {
        return null;
    }

    return {
        part_number: deviceData.part_number,
        package: deviceData.selected_package,
        assignments,
        signal_names: signalNames,
        reserved_assignments: {
            jtag: jtagReservedAssignments,
            i2c: i2cRoutedAssignments,
        },
        oscillator: getOscConfig(),
        fuses: getFuseConfig(),
        clc: getClcConfig(),
    };
}

function serializeConfigPayload(payload) {
    return JSON.stringify(payload, null, 2);
}

function currentConfigContents() {
    const payload = buildCurrentConfigPayload();
    return payload ? serializeConfigPayload(payload) : null;
}

function shortenConfigPath(path, maxLength = 72) {
    const rawPath = String(path || '').trim();
    if (!rawPath) {
        return '';
    }

    const normalized = rawPath.replace(/\\/g, '/');
    let shortened = normalized;

    shortened = shortened.replace(/^\/Users\/[^/]+(?=\/)/, '~');
    shortened = shortened.replace(/^\/home\/[^/]+(?=\/)/, '~');
    shortened = shortened.replace(/^[A-Za-z]:\/Users\/[^/]+(?=\/)/, '~');

    if (shortened.length <= maxLength) {
        return shortened;
    }

    const segments = shortened.split('/').filter(Boolean);
    if (segments.length <= 2) {
        return shortened;
    }

    const filename = segments[segments.length - 1];
    const parent = segments[segments.length - 2];
    const root = shortened.startsWith('~/')
        ? '~'
        : (normalized.startsWith('/') ? '/' : segments[0]);

    const compact = root === '/'
        ? `/.../${parent}/${filename}`
        : `${root}/.../${parent}/${filename}`;

    if (compact.length <= maxLength) {
        return compact;
    }

    const available = Math.max(12, maxLength - (`${root}/.../${parent}/`.length));
    if (filename.length <= available) {
        return compact;
    }

    return `${root}/.../${parent}/...${filename.slice(-(available - 3))}`;
}

function currentTauriWindowHandle() {
    const tauriWindow = window.__TAURI__?.window;
    if (tauriWindow?.getCurrentWindow) {
        return tauriWindow.getCurrentWindow();
    }
    if (tauriWindow?.appWindow) {
        return tauriWindow.appWindow;
    }

    const webviewWindow = window.__TAURI__?.webviewWindow;
    if (webviewWindow?.getCurrentWebviewWindow) {
        return webviewWindow.getCurrentWebviewWindow();
    }
    if (webviewWindow?.getCurrent) {
        return webviewWindow.getCurrent();
    }

    return null;
}

function applyWindowTitle(title) {
    if (!title || title === lastWindowTitle) {
        return;
    }

    lastWindowTitle = title;
    document.title = title;

    try {
        const handle = currentTauriWindowHandle();
        if (handle?.setTitle) {
            Promise.resolve(handle.setTitle(title)).catch(() => {});
        }
    } catch (_) {
        // Keep the DOM title updated even if the native API is unavailable.
    }
}

function currentConfigTitleLabel() {
    return configDocument.path
        ? shortenConfigPath(configDocument.path)
        : appConfig.ui.configFiles.unsavedName;
}

function configDocumentDirty() {
    const currentContents = currentConfigContents();
    return Boolean(
        deviceData
        && currentContents !== null
        && configDocument.savedContents !== null
        && currentContents !== configDocument.savedContents
    );
}

function refreshConfigDocumentUi() {
    const saveButton = $('save-btn');
    const saveAsButton = $('save-as-btn');
    const renameButton = $('rename-btn');
    const saveMenuButton = $('save-menu-btn');
    const hasDevice = Boolean(deviceData);
    const hasPath = Boolean(configDocument.path);
    const dirty = configDocumentDirty();
    const titleLabel = currentConfigTitleLabel();
    const configFileUi = appConfig.ui.configFiles;

    if (saveButton) {
        saveButton.disabled = !hasDevice;
        saveButton.textContent = configFileUi.saveButton;
        saveButton.title = configFileUi.saveShortcutHint;
    }
    if (saveAsButton) {
        saveAsButton.textContent = configFileUi.saveAsButton;
        saveAsButton.disabled = !hasDevice;
    }
    if (renameButton) {
        renameButton.textContent = configFileUi.renameButton;
        renameButton.disabled = !hasPath;
    }
    if (saveMenuButton) {
        saveMenuButton.title = configFileUi.moreActionsTitle;
    }

    if (!hasDevice && !hasPath) {
        applyWindowTitle('pickle — Pin Configurator');
        return;
    }

    applyWindowTitle(dirty
        ? `pickle — Pin Configurator — ${titleLabel} *`
        : `pickle — Pin Configurator — ${titleLabel}`);
}

function initializeConfigDocumentUi() {
    refreshConfigDocumentUi();
}

function markConfigDocumentSaved(path, savedContents) {
    configDocument.path = path || null;
    configDocument.savedContents = savedContents ?? currentConfigContents();
    refreshConfigDocumentUi();
}

function markConfigDocumentDirty() {
    refreshConfigDocumentUi();
}

function syncConfigDocumentAfterDeviceLoad(options = {}) {
    if (!options.preserveState) {
        markConfigDocumentSaved(null);
        return;
    }
    if (options.markDirty) {
        markConfigDocumentDirty();
        return;
    }
    refreshConfigDocumentUi();
}

// =============================================================================
// Save / Load Configuration
// =============================================================================

function suggestedConfigFilename() {
    if (configDocument.path) {
        return configFileBasename(configDocument.path);
    }
    if (!deviceData) {
        return 'pin_config.json';
    }
    return `${deviceData.part_number}_${deviceData.selected_package}.json`;
}

function currentConfigFileText() {
    return currentConfigContents() || '';
}

async function saveConfigAs() {
    if (!deviceData) return;
    const contents = currentConfigFileText();
    const title = configDocument.path
        ? appConfig.ui.configFiles.saveAsDialogTitle
        : appConfig.ui.configFiles.saveDialogTitle;
    try {
        const result = await invoke('save_text_file_dialog', {
            request: {
                title,
                suggestedName: suggestedConfigFilename(),
                contents,
                filters: [{ name: 'JSON', extensions: ['json'] }],
            },
        });
        if (result) {
            markConfigDocumentSaved(result.path, contents);
            setStatus(`Saved config to ${result.path}`, 'success');
        }
    } catch (e) {
        setStatus('Error saving config: ' + (e.message || e), 'error');
    }
}

/** Save the current configuration to its current file path, or fall back to Save As. */
async function saveConfig() {
    if (!deviceData) return;
    if (!configDocument.path) {
        await saveConfigAs();
        return;
    }

    const contents = currentConfigFileText();
    try {
        const result = await invoke('write_text_file_path', {
            path: configDocument.path,
            contents,
        });
        markConfigDocumentSaved(result.path, contents);
        setStatus(`Saved config to ${result.path}`, 'success');
    } catch (e) {
        setStatus('Error saving config: ' + (e.message || e), 'error');
    }
}

/** Save under a new name and remove the previous on-disk file when possible. */
async function renameConfig() {
    if (!deviceData) return;
    if (!configDocument.path) {
        await saveConfigAs();
        return;
    }

    const oldPath = configDocument.path;
    const contents = currentConfigFileText();

    try {
        const result = await invoke('save_text_file_dialog', {
            request: {
                title: appConfig.ui.configFiles.renameDialogTitle,
                suggestedName: suggestedConfigFilename(),
                contents,
                filters: [{ name: 'JSON', extensions: ['json'] }],
            },
        });
        if (!result) return;

        markConfigDocumentSaved(result.path, contents);

        if (result.path !== oldPath) {
            try {
                await invoke('delete_file_path', { path: oldPath });
            } catch (deleteError) {
                setStatus(`Renamed config to ${result.path} (old file kept: ${deleteError.message || deleteError})`, 'warn');
                return;
            }
        }

        setStatus(`Renamed config to ${result.path}`, 'success');
    } catch (e) {
        setStatus('Error renaming config: ' + (e.message || e), 'error');
    }
}

/** Open a previously saved configuration JSON file via a native file picker. */
async function openConfigDialog() {
    try {
        const result = await invoke('open_text_file_dialog', {
            request: {
                title: appConfig.ui.configFiles.openDialogTitle,
                filters: [{ name: 'JSON', extensions: ['json'] }],
            },
        });
        if (!result) return;
        await loadConfigText(result.contents, result.path);
    } catch (e) {
        setStatus('Error loading config: ' + (e.message || e), 'error');
    }
}

/**
 * Restore a configuration JSON string and reload the selected device.
 * @param {string} text - JSON configuration text
 * @param {string} [sourcePath] - Optional source path shown in the status text
 */
async function loadConfigText(text, sourcePath) {
    try {
        const config = JSON.parse(text);

        if (!config.part_number) {
            setStatus('Invalid config file: missing part_number', 'error');
            return;
        }

        $('part-input').value = config.part_number;

        // Seed state before rendering so the freshly loaded device paints the saved config
        // during the first render instead of flashing an empty assignment table.
        assignments = model.normalizePositionMap(config.assignments);
        signalNames = model.normalizePositionMap(config.signal_names);
        jtagReservedAssignments = model.normalizePositionMap(config.reserved_assignments?.jtag);
        i2cRoutedAssignments = model.normalizePositionMap(config.reserved_assignments?.i2c);

        applyClcConfig(config.clc);

        await loadDevice(config.package || null, { preserveState: true });
        applyOscillatorConfig(config.oscillator);
        applyFuseSelections(config.fuses?.selections);
        markConfigDocumentSaved(sourcePath || null);
        const sourceName = sourcePath ? ` from ${sourcePath.split(/[\\/]/).pop()}` : '';
        setStatus(`Loaded config${sourceName}: ${config.part_number} — ${config.package || 'default'}`, 'success');
    } catch (e) {
        setStatus('Error loading config: ' + (e.message || e), 'error');
    }
}
