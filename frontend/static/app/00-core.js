/**
 * pickle — Pin Configurator Frontend
 *
 * Single-page application for dsPIC33 and PIC24 pin multiplexing configuration.
 * Manages device data, pin assignments, code generation, and UI state.
 *
 * Architecture:
 *   - State: global variables (deviceData, assignments, signalNames, fuse-driven stash maps)
 *   - Rendering: imperative DOM manipulation (no framework)
 *   - Backend: Tauri IPC via invoke()
 *   - File I/O: native Tauri dialogs handled by Rust commands
 *   - Undo: snapshot-based (structuredClone of state objects)
 */

const { invoke } = window.__TAURI__.core;
const appConfig = window.PickleConfig;
const model = window.PickleModel;

function $(id) {
    return document.getElementById(id);
}

function resolveElement(target) {
    return typeof target === 'string' ? $(target) : target;
}

function setVisibility(target, visible, display = '') {
    const el = resolveElement(target);
    if (el) {
        el.style.display = visible ? display : 'none';
    }
    return el;
}

function showElement(target, display = '') {
    return setVisibility(target, true, display);
}

function hideElement(target) {
    return setVisibility(target, false);
}

function setTextContent(target, text) {
    const el = resolveElement(target);
    if (el) {
        el.textContent = text;
    }
    return el;
}

function flashButtonLabel(buttonId, activeLabel, idleLabel, durationMs = appConfig.ui.timings.buttonFlashMs) {
    const btn = $(buttonId);
    if (!btn) return;
    btn.textContent = activeLabel;
    setTimeout(() => {
        btn.textContent = idleLabel;
    }, durationMs);
}

// =============================================================================
// State
// =============================================================================

/** @type {Object|null} Device data from invoke('load_device') */
let deviceData = null;

/** @type {Object.<number, {peripheral:string, direction:string, ppsval:?number, rp_number:?number, fixed:boolean}>} */
let assignments = {};

/** @type {Object.<number, string>} Pin position -> user-defined signal name */
let signalNames = {};

/** @type {Object.<number, {peripheral:string, direction:string, ppsval:?number, rp_number:?number, fixed:boolean}>} */
let jtagReservedAssignments = {};

/** @type {Object.<number, {assignment:{peripheral:string, direction:string, ppsval:?number, rp_number:?number, fixed:boolean}, signalName:string}>} */
let i2cRoutedAssignments = {};

/** @type {Object.<string, string>} Generated file contents keyed by filename */
let generatedFiles = {};

/** @type {string} Currently visible code tab */
let activeTab = model.generatedSourceFilename(defaultAppSettings());

/** @type {{total:number, cached:number, available:boolean, ageHours:?number, isStale:boolean}} */
let indexCatalogState = {
    total: 0,
    cached: 0,
    available: false,
    ageHours: null,
    isStale: true,
};

/** @type {{appearance:{theme:string}, startup:{device:string, package:string}, toolchain:{fallback_compiler:string, family_compilers:{pic24:string, dspic33:string}}, codegen:{output_basename:string}, verification:{provider:string}, onboarding:{welcome_intro_seen:boolean}, last_used:{part_number:string, package:string}}} */
let appSettings = defaultAppSettings();

function defaultAppSettings() {
    return model.defaultAppSettings();
}

// =============================================================================
// Undo / Redo
// =============================================================================

/** @type {Array<{assignments:Object, signalNames:Object}>} */
let undoStack = [];
/** @type {Array<{assignments:Object, signalNames:Object}>} */
let redoStack = [];
const MAX_UNDO = appConfig.ui.undo.maxSnapshots;

/** Reset per-device editor state when loading a different part. */
function resetEditorState() {
    assignments = {};
    signalNames = {};
    jtagReservedAssignments = {};
    i2cRoutedAssignments = {};
    undoStack = [];
    redoStack = [];
    periphCardState = {};
}

/** Snapshot current state onto the undo stack. Clears redo stack. */
function pushUndo() {
    undoStack.push({
        assignments: structuredClone(assignments),
        signalNames: structuredClone(signalNames),
    });
    if (undoStack.length > MAX_UNDO) undoStack.shift();
    redoStack = [];
}

/** Restore the previous state from the undo stack. */
function undo() {
    if (!undoStack.length || !deviceData) return;
    redoStack.push({
        assignments: structuredClone(assignments),
        signalNames: structuredClone(signalNames),
    });
    const state = undoStack.pop();
    assignments = state.assignments;
    signalNames = state.signalNames;
    renderCurrentEditorView();
    checkConflicts();
    markConfigDocumentDirty();
}

/** Re-apply the last undone state from the redo stack. */
function redo() {
    if (!redoStack.length || !deviceData) return;
    undoStack.push({
        assignments: structuredClone(assignments),
        signalNames: structuredClone(signalNames),
    });
    const state = redoStack.pop();
    assignments = state.assignments;
    signalNames = state.signalNames;
    renderCurrentEditorView();
    checkConflicts();
    markConfigDocumentDirty();
}

/** Global keyboard shortcut handler for save (Cmd/Ctrl+S), undo, and redo. */
document.addEventListener('keydown', (e) => {
    const key = String(e.key || '').toLowerCase();
    if ((e.ctrlKey || e.metaKey) && key === 's') {
        e.preventDefault();
        void saveConfig();
        return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'z' && !e.shiftKey) {
        e.preventDefault();
        undo();
    }
    if ((e.ctrlKey || e.metaKey) && ((e.key === 'z' && e.shiftKey) || e.key === 'y')) {
        e.preventDefault();
        redo();
    }
});

// =============================================================================
// Analog Pin Sharing — Classification & Assignment Helpers
// =============================================================================

/**
 * Check if a peripheral name is a passive analog input that can share a pin
 * with other analog inputs (comparator inputs, ADC channels, op-amp inputs,
 * voltage references).
 */
function isAnalogInput(name) {
    return model.isAnalogInput(name);
}

/**
 * Check if a peripheral name is an analog output (op-amp output, DAC output).
 * Analog outputs drive the pin, so at most one can be active, but analog inputs
 * can still share the pin (they read the driven value).
 */
function isAnalogOutput(name) {
    return model.isAnalogOutput(name);
}

/** Check if a peripheral is any analog function (input or output). */
function isAnalogFunction(name) {
    return model.isAnalogFunction(name);
}

/**
 * Get all assignments at a pin position, always as an array.
 * Handles both single-assignment (legacy) and multi-assignment (array) forms.
 * @param {number} pos - Pin position
 * @returns {Array<{peripheral:string, direction:string, ppsval:?number, rp_number:?number, fixed:boolean}>}
 */
function getAssignmentsAt(pos) {
    return model.getAssignmentsAt(assignments, pos);
}

/**
 * Set a single exclusive assignment at a pin position (replaces all existing).
 * Use for PPS, digital, and GPIO assignments.
 */
function setAssignment(pos, assignment) {
    assignments[pos] = assignment;
}

/**
 * Add an analog assignment to a pin, preserving existing analog assignments.
 * If the pin already has a non-analog (digital/PPS) assignment, it replaces it.
 */
function addAnalogAssignment(pos, assignment) {
    const existing = getAssignmentsAt(pos);

    // If pin has no assignments, just set it
    if (existing.length === 0) {
        assignments[pos] = assignment;
        return;
    }

    // If all existing assignments are analog, append
    const allAnalog = existing.every(a => isAnalogFunction(a.peripheral));
    if (allAnalog) {
        // Don't add duplicates
        if (existing.some(a => a.peripheral === assignment.peripheral)) return;
        assignments[pos] = [...existing, assignment];
    } else {
        // Replace non-analog assignment
        assignments[pos] = assignment;
    }
}

/**
 * Remove a specific peripheral assignment from a pin position.
 * Handles both single and multi-assignment forms.
 * @returns {boolean} true if something was removed
 */
function removeAssignment(pos, peripheralName) {
    const val = assignments[pos];
    if (!val) return false;

    if (Array.isArray(val)) {
        const filtered = val.filter(a => a.peripheral !== peripheralName);
        if (filtered.length === val.length) return false;
        if (filtered.length === 0) {
            delete assignments[pos];
        } else if (filtered.length === 1) {
            assignments[pos] = filtered[0]; // unwrap single-element array
        } else {
            assignments[pos] = filtered;
        }
        return true;
    }

    if (val.peripheral === peripheralName) {
        delete assignments[pos];
        return true;
    }
    return false;
}

/**
 * Check if a specific peripheral is assigned at a pin position.
 */
function hasAssignmentFor(pos, peripheralName) {
    return model.hasAssignmentFor(assignments, pos, peripheralName);
}

/**
 * Get the primary (first) assignment at a pin position, or null.
 * Used for display when a single representative is needed.
 */
function primaryAssignment(pos) {
    return model.primaryAssignment(assignments, pos);
}

function forEachAssignedPin(callback) {
    model.forEachAssignedPin(assignments, callback);
}

function flattenAssignments() {
    return model.flattenAssignments(assignments);
}

// =============================================================================
// Peripheral Classification
// =============================================================================

/**
 * Return a CSS class name for color-coding a peripheral by type.
 * @param {string} name - Peripheral name (e.g. "U1TX", "SDA1", "AN0")
 * @returns {string} CSS class name
 */
function periphClass(name) {
    if (/^U\d/.test(name)) return 'periph-uart';
    if (/^S[DC][OI]|^SS|^SCK/.test(name)) return 'periph-spi';
    if (/^S[DC][AL]/.test(name)) return 'periph-i2c';
    if (/^PWM|^OCM/.test(name)) return 'periph-pwm';
    if (/^T\d|^TCKI|^ICM/.test(name)) return 'periph-timer';
    if (/^AN|^ADC|^AD\d+AN/.test(name)) return 'periph-adc';
    if (/^CMP/.test(name)) return 'periph-cmp';
    if (/^OA/.test(name)) return 'periph-opamp';
    return 'periph-other';
}

/**
 * Return a granular CSS color class for a function tag.
 * More specific than periphClass — gives each peripheral type its own color.
 * @param {string} name - Function name (e.g. "AN0", "CMP1D", "OA2OUT", "RB2")
 * @returns {string} CSS class name (e.g. "tag-adc", "tag-cmp")
 */
function funcTagColorClass(name) {
    if (/^AN[A-Z]?\d+$|^AD\d+AN[A-Z]*\d+$|^VREF[+-]?$|^IBIAS/.test(name)) return 'tag-adc';
    if (/^CMP\d+[A-D]?$/.test(name))                     return 'tag-cmp';
    if (/^OA\d/.test(name))                                return 'tag-opamp';
    if (/^DAC/.test(name))                                 return 'tag-dac';
    if (/^U\d/.test(name))                                 return 'tag-uart';
    if (/^S[DC][OI]\d|^SS\d|^SCK\d/.test(name))           return 'tag-spi';
    if (/^A?S[DC][AL]\d/.test(name))                       return 'tag-i2c';
    if (/^PWM|^OCM|^PCI\d/.test(name))                     return 'tag-pwm';
    if (/^T\d|^TCKI|^ICM/.test(name))                      return 'tag-timer';
    if (/^R[A-Z]\d+$/.test(name))                          return 'tag-gpio';
    if (/^INT\d/.test(name))                                return 'tag-int';
    if (/^CLC|^SENT|^QE|^HOME|^INDX|^REF[IO]|^OSC|^CLK|^C\d+(TX|RX)$/.test(name)) return 'tag-sys';
    if (/^PG[DC]\d|^MCLR/.test(name))                      return 'tag-debug';
    if (/^TD[IO]$|^TMS$|^TCK$/.test(name))                 return 'tag-jtag';
    if (/^RP\d+$/.test(name))                               return 'tag-pps';
    return 'tag-other';
}

/**
 * Return a human-readable group name for a peripheral (used in dropdown optgroups).
 * @param {string} name - Peripheral name
 * @returns {string} Group label
 */
function periphGroup(name) {
    if (/^U\d/.test(name)) return 'UART';
    if (/^S[DC][OI]|^SS|^SCK/.test(name)) return 'SPI';
    if (/^S[DC][AL]/.test(name)) return 'I2C';
    if (/^PWM|^OCM/.test(name)) return 'PWM/OC';
    if (/^T\d|^TCKI|^ICM/.test(name)) return 'Timer/IC';
    if (/^CMP/.test(name)) return 'Comparator';
    if (/^REFO?$/.test(name)) return 'Reference';
    if (/^INT\d/.test(name)) return 'Interrupt';
    if (/^QEA/.test(name)) return 'QEI';
    if (/^SENT/.test(name)) return 'SENT';
    if (/^PCI/.test(name)) return 'PWM PCI';
    if (/^CLC/.test(name)) return 'CLC';
    return 'Other';
}

// =============================================================================
// Device Loading
// =============================================================================

/**
 * Load device data from the backend and render the UI.
 * @param {string} [pkg] - Optional package name. If omitted, loads default package.
 * @param {{preserveState?: boolean, markDirty?: boolean}} [options] - Preserve assignments/signal names when true.
 *                                                Used for package switches and config restore.
 */
async function loadDevice(pkg, options = {}) {
    const part = $('part-input').value.trim().toUpperCase();
    if (!part) return;

    const preserveState = options.preserveState ?? Boolean(pkg);
    const isCached = cachedDevices.has(part);
    setStatus(isCached ? appConfig.ui.deviceLoad.cachedStatus : appConfig.ui.deviceLoad.remoteStatus, 'busy');

    try {
        deviceData = await invoke('load_device', { partNumber: part, package: pkg || null });

        const preferredSelectedPackage = model.preferredVisiblePackageName(
            deviceData?.packages || {},
            deviceData?.selected_package || ''
        );
        if (
            preferredSelectedPackage
            && deviceData?.selected_package
            && preferredSelectedPackage !== deviceData.selected_package
        ) {
            await loadDevice(preferredSelectedPackage, options);
            return;
        }

        // Always show the current package once a device is loaded. Even when
        // there is only one visible package, the selector is still useful as
        // an identity anchor for the attached package-actions menu.
        const pkgSelect = $('pkg-select');
        const pkgEntries = visiblePackageEntries();
        const pkgGroup = $('pkg-control-group');
        if (pkgSelect && pkgEntries.length > 0) {
            pkgSelect.innerHTML = '';
            for (const entry of pkgEntries) {
                const name = entry.backendKey;
                const meta = deviceData.packages[name];
                const opt = document.createElement('option');
                opt.value = name;
                opt.textContent = `${entry.displayName} (${meta.pin_count}p)`;
                if (entry.synthetic) {
                    opt.title = 'Fallback package from the EDC. This is not a real package name; verify against the datasheet to import the actual package.';
                } else if (entry.displayName !== name) {
                    opt.title = `Stored package name: ${name}`;
                }
                if (name === deviceData.selected_package) opt.selected = true;
                pkgSelect.appendChild(opt);
            }
            pkgSelect.disabled = false;
            pkgSelect.title = isSyntheticPackage(deviceData.selected_package)
                ? 'Current package is a fallback from the EDC and not a real package name. Verify against the datasheet to import the actual package.'
                : 'Current package';
            showElement(pkgGroup, 'inline-flex');
        } else {
            if (pkgSelect) {
                pkgSelect.disabled = false;
            }
            hideElement(pkgGroup);
        }
        refreshPackageManagerUi();

        // Show verify button when device is loaded
        showElement('verify-btn');

        // Reset editor state only when switching to a different part or starting fresh.
        if (!preserveState) {
            resetEditorState();
        }

        // Show configuration panels
        showElement('save-action-group', 'flex');
        showElement('load-btn-file');
        showElement('osc-config');
        hideElement('fuses-empty');
        buildFuseUI(deviceData.fuse_defs);

        // Initialize CLC designer
        if (!preserveState) initClcConfig();
        updateClcTabState();
        renderClcDesigner();

        // Populate Device Info tab
        renderDeviceInfo();

        // Show the view toggle once a device is loaded
        showElement('view-toggle');
        await checkCompiler(deviceData.part_number);

        renderCurrentEditorView();
        syncConfigDocumentAfterDeviceLoad({ preserveState, markDirty: options.markDirty });
        if (isSyntheticPackage(deviceData.selected_package)) {
            setStatus(`${deviceData.part_number} — ${displayPackageName(deviceData.selected_package, { long: true })}. Verify against the datasheet to import the actual package.`, 'warn');
        } else {
            setStatus(`${deviceData.part_number} — ${displayPackageName(deviceData.selected_package, { long: true })}`, 'success');
        }

        if (typeof dismissWelcomeIntro === 'function') {
            dismissWelcomeIntro({ persist: true });
        }

        // Update cached device set if this was a new download
        if (!isCached) {
            cachedDevices.add(part);
            populateDeviceList();
        }

        try {
            await rememberLastUsedDevice(deviceData.part_number, deviceData.selected_package);
        } catch (settingsError) {
            console.warn('Failed to remember last-used device:', settingsError);
        }
    } catch (e) {
        hideElement('pkg-control-group');
        if (typeof closePackageMenu === 'function') {
            closePackageMenu();
        }
        setStatus('Error: ' + (e.message || e), 'error');
    }
}

/** Update the bottom status bar with an explicit tone. */
function setStatus(msg, tone) {
    window.PickleUI.status(msg == null || msg === '' ? 'Ready' : msg, tone || 'idle');
}

function packageMeta(name) {
    if (!deviceData?.packages || !name) {
        return null;
    }
    return deviceData.packages[name] || null;
}

function isSyntheticPackage(name, meta = packageMeta(name)) {
    return !!(name && /^default$/i.test(String(name).trim()) && meta?.source === 'edc');
}

function selectedPackageMeta() {
    return packageMeta(deviceData?.selected_package || '');
}

function selectedPackageIsOverlay() {
    return selectedPackageMeta()?.source === 'overlay';
}

function normalizeFriendlyPackageName(name) {
    return model.normalizeFriendlyPackageName(name);
}

function packageSourceLabel(meta = selectedPackageMeta()) {
    return meta?.source === 'overlay'
        ? appConfig.ui.packageManager.sourceOverlay
        : appConfig.ui.packageManager.sourceBuiltin;
}

function packageDefaultDisplayName(name, options = {}) {
    const meta = options.meta || packageMeta(name);
    return model.packageIdentity(name, meta).defaultDisplayName;
}

function editablePackageName(name, options = {}) {
    const meta = options.meta || packageMeta(name);
    const explicitDisplayName = String(options.displayName ?? '').trim();
    if (explicitDisplayName) {
        return explicitDisplayName;
    }
    return model.packageIdentity(name, meta).displayName;
}

function hasPackageDisplayNameOverride(name, meta = packageMeta(name)) {
    return Boolean(String(meta?.display_name || '').trim());
}

function displayPackageName(name, options = {}) {
    const meta = options.meta || packageMeta(name);
    if (!name) {
        return '—';
    }
    const baseName = editablePackageName(name, {
        meta,
        displayName: options.displayName,
    });
    if (!isSyntheticPackage(name, meta)) {
        return baseName;
    }
    return options.long
        ? `${baseName} [not a real package]`
        : `${baseName} [not real]`;
}

function refreshPackageManagerUi() {
    const group = $('pkg-control-group');
    const button = $('pkg-menu-btn');
    if (!group || !button) {
        return;
    }

    if (!deviceData?.selected_package) {
        hideElement(group);
        if (typeof closePackageMenu === 'function') {
            closePackageMenu();
        }
        return;
    }

    button.textContent = appConfig.ui.packageManager.menuButtonLabel;
    button.title = appConfig.ui.packageManager.menuButtonTitle;
    showElement(group, 'inline-flex');

    // Dropdown factory rebuilds menu items on each open from current
    // overlay/override state, so no eager sync is needed here.

    const dialog = $('package-dialog');
    if (dialog?.open) {
        populatePackageManagerDialog();
    }
}

function refreshPackageManagerActionState() {
    const nameInput = $('package-name-input');
    const saveButton = $('package-save-btn');
    const resetButton = $('package-reset-btn');
    const deleteButton = $('package-delete-btn');
    if (!nameInput || !saveButton || !resetButton || !deleteButton) {
        return;
    }

    const currentPackage = deviceData?.selected_package || '';
    const meta = selectedPackageMeta();
    const currentEditableName = editablePackageName(currentPackage, { meta });
    const nextName = nameInput.value.trim();
    const canEditName = Boolean(currentPackage);

    nameInput.disabled = !canEditName;
    saveButton.disabled = !canEditName || !nextName || nextName === currentEditableName;
    resetButton.disabled = !canEditName || !hasPackageDisplayNameOverride(currentPackage, meta);
    deleteButton.disabled = !selectedPackageIsOverlay();
}

function populatePackageManagerDialog() {
    const currentPackage = deviceData?.selected_package || '';
    const meta = selectedPackageMeta();
    const currentDisplay = displayPackageName(currentPackage, { long: true });
    const currentSource = packageSourceLabel(meta);
    const ui = appConfig.ui.packageManager;

    setTextContent('package-dialog-title', ui.dialogTitle);
    setTextContent('package-dialog-current-label', ui.currentLabel);
    setTextContent('package-dialog-stored-label', ui.storedLabel);
    setTextContent('package-dialog-source-label', ui.sourceLabel);
    setTextContent('package-dialog-name-label', ui.nameLabel);
    setTextContent('package-close-btn', ui.closeButton);
    setTextContent('package-cancel-btn', ui.closeButton);
    setTextContent('package-save-btn', ui.saveButton);
    setTextContent('package-reset-btn', ui.resetButton);
    setTextContent('package-delete-btn', ui.deleteButton);
    setTextContent('package-dialog-current', currentDisplay);
    setTextContent('package-dialog-source', currentSource);
    setTextContent('package-dialog-note', selectedPackageIsOverlay() ? ui.overlayNote : ui.builtinNote);

    const storedRow = $('package-dialog-stored-row');
    const storedValue = $('package-dialog-stored');
    if (storedRow && storedValue) {
        if (currentDisplay !== currentPackage) {
            storedValue.textContent = currentPackage;
            storedRow.hidden = false;
        } else {
            storedValue.textContent = '';
            storedRow.hidden = true;
        }
    }

    const nameInput = $('package-name-input');
    if (nameInput) {
        nameInput.placeholder = ui.namePlaceholder;
        nameInput.value = editablePackageName(currentPackage, { meta });
    }
    refreshPackageManagerActionState();
}

function showPackageManagerDialog() {
    if (!deviceData?.selected_package) {
        return;
    }

    if (typeof closePackageMenu === 'function') {
        closePackageMenu();
    }

    const dialog = $('package-dialog');
    if (!dialog || dialog.open) {
        return;
    }

    populatePackageManagerDialog();
    dialog.showModal();
    $('package-name-input')?.focus();
    $('package-name-input')?.select();
}

function closePackageManagerDialog() {
    $('package-dialog')?.close();
}

function syncVerificationPackageDelete(packageName) {
    if (typeof verifyResult === 'undefined' || !verifyResult?.packages?.[packageName]) {
        return;
    }

    delete verifyResult.packages[packageName];
    if (typeof renderVerifyResult === 'function') {
        renderVerifyResult(verifyResult);
    }
}

async function saveSelectedPackageDisplayName() {
    if (!deviceData?.part_number || !deviceData?.selected_package) {
        return;
    }

    const meta = selectedPackageMeta();
    const packageName = deviceData.selected_package;
    const defaultDisplayName = packageDefaultDisplayName(packageName, { meta });
    const currentEditableName = editablePackageName(packageName, { meta });
    const nameInput = $('package-name-input');
    const nextName = String(nameInput?.value || '').trim();
    if (!nextName || nextName === currentEditableName) {
        refreshPackageManagerActionState();
        return;
    }

    const displayName = nextName === defaultDisplayName ? null : nextName;

    try {
        const result = await invoke('set_package_display_name', {
            request: {
                partNumber: deviceData.part_number,
                packageName,
                displayName,
            },
        });
        await loadDevice(result.packageName || packageName, { preserveState: true, markDirty: true });
        setStatus(displayName
            ? appConfig.format(appConfig.ui.packageManager.savedStatus, {
                packageName: displayName,
            })
            : appConfig.format(appConfig.ui.packageManager.resetStatus, {
                packageName: defaultDisplayName,
            }), 'success');
        closePackageManagerDialog();
    } catch (error) {
        setStatus(`Error saving package name: ${error.message || error}`, 'error');
    }
}

async function resetSelectedPackageDisplayName() {
    if (!deviceData?.part_number || !deviceData?.selected_package) {
        return;
    }

    const meta = selectedPackageMeta();
    const packageName = deviceData.selected_package;
    if (!hasPackageDisplayNameOverride(packageName, meta)) {
        refreshPackageManagerActionState();
        return;
    }

    const defaultDisplayName = packageDefaultDisplayName(packageName, { meta });

    try {
        const result = await invoke('set_package_display_name', {
            request: {
                partNumber: deviceData.part_number,
                packageName,
                displayName: null,
            },
        });
        await loadDevice(result.packageName || packageName, { preserveState: true, markDirty: true });
        setStatus(appConfig.format(appConfig.ui.packageManager.resetStatus, {
            packageName: defaultDisplayName,
        }), 'success');
        closePackageManagerDialog();
    } catch (error) {
        setStatus(`Error resetting package name: ${error.message || error}`, 'error');
    }
}

async function deleteSelectedOverlayPackage() {
    if (!deviceData?.part_number || !selectedPackageIsOverlay()) {
        return;
    }

    const packageName = deviceData.selected_package;
    const confirmed = window.confirm(appConfig.format(appConfig.ui.packageManager.deleteConfirm, {
        packageName: displayPackageName(packageName, { long: true }),
    }));
    if (!confirmed) {
        return;
    }

    try {
        await invoke('delete_overlay_package', {
            request: {
                partNumber: deviceData.part_number,
                packageName,
            },
        });
        syncVerificationPackageDelete(packageName);
        closePackageManagerDialog();
        await loadDevice(undefined, { preserveState: true, markDirty: true });
        setStatus(appConfig.format(appConfig.ui.packageManager.deletedStatus, {
            packageName: displayPackageName(packageName, { long: true }),
        }), 'success');
    } catch (error) {
        setStatus(`Error deleting overlay package: ${error.message || error}`, 'error');
    }
}

function wirePackageManagerDialog() {
    const dialog = $('package-dialog');
    if (!dialog || dialog.dataset.bound === 'true') {
        return;
    }
    dialog.dataset.bound = 'true';

    dialog.addEventListener('click', (event) => {
        const rect = dialog.getBoundingClientRect();
        if (
            event.clientX < rect.left
            || event.clientX > rect.right
            || event.clientY < rect.top
            || event.clientY > rect.bottom
        ) {
            dialog.close();
        }
    });

    $('package-name-input')?.addEventListener('input', refreshPackageManagerActionState);
    $('package-name-input')?.addEventListener('keydown', (event) => {
        if (event.key === 'Enter') {
            event.preventDefault();
            void saveSelectedPackageDisplayName();
        }
    });
    $('package-close-btn')?.addEventListener('click', closePackageManagerDialog);
    $('package-cancel-btn')?.addEventListener('click', closePackageManagerDialog);
    $('package-save-btn')?.addEventListener('click', () => {
        void saveSelectedPackageDisplayName();
    });
    $('package-reset-btn')?.addEventListener('click', () => {
        void resetSelectedPackageDisplayName();
    });
    $('package-delete-btn')?.addEventListener('click', () => {
        void deleteSelectedOverlayPackage();
    });
}

function visiblePackageNames() {
    return model.visiblePackageNames(deviceData?.packages || {});
}

function visiblePackageEntries() {
    return model.visiblePackageEntries(deviceData?.packages || {});
}

function renderDeviceSummary() {
    if (!deviceData) return;
    const rpPins = deviceData.pins.filter(pin => pin.rp_number !== null);
    setTextContent('sum-pins', deviceData.pin_count);
    setTextContent('sum-rp', rpPins.length);
    setTextContent('sum-pkg', displayPackageName(deviceData.selected_package, { long: true }));
    showElement('summary');
}

// =============================================================================
// Behavior Settings
// =============================================================================

/** Normalize theme mode to the values supported by the UI and settings file. */
function normalizeThemeMode(mode) {
    return model.normalizeThemeMode(mode);
}

/**
 * Load the persisted behavior settings. The backend owns file creation and
 * serialization so the frontend can treat the result as canonical state.
 */
async function loadAppSettings() {
    try {
        const data = await invoke('load_app_settings');
        const settings = data?.settings || defaultAppSettings();

        appSettings = {
            appearance: {
                theme: normalizeThemeMode(String(settings.appearance?.theme || 'dark').trim().toLowerCase()),
            },
            startup: {
                device: String(settings.startup?.device || 'last-used').trim(),
                package: String(settings.startup?.package || '').trim(),
            },
            toolchain: {
                fallback_compiler: String(settings.toolchain?.fallback_compiler || settings.toolchain?.compiler || defaultAppSettings().toolchain.fallback_compiler).trim()
                    || defaultAppSettings().toolchain.fallback_compiler,
                family_compilers: {
                    pic24: String(settings.toolchain?.family_compilers?.pic24 || defaultAppSettings().toolchain.family_compilers.pic24).trim()
                        || defaultAppSettings().toolchain.family_compilers.pic24,
                    dspic33: String(settings.toolchain?.family_compilers?.dspic33 || defaultAppSettings().toolchain.family_compilers.dspic33).trim()
                        || defaultAppSettings().toolchain.family_compilers.dspic33,
                },
            },
            codegen: {
                output_basename: model.normalizeOutputBasename(
                    settings.codegen?.output_basename || defaultAppSettings().codegen.output_basename
                ),
            },
            verification: {
                provider: model.normalizeVerificationProvider(
                    settings.verification?.provider || defaultAppSettings().verification.provider
                ),
            },
            onboarding: {
                welcome_intro_seen: Boolean(
                    settings.onboarding?.welcome_intro_seen
                    ?? defaultAppSettings().onboarding.welcome_intro_seen
                ),
            },
            last_used: {
                part_number: String(settings.last_used?.part_number || '').trim().toUpperCase(),
                package: String(settings.last_used?.package || '').trim(),
            },
        };

        if (Object.keys(generatedFiles).length === 0) {
            activeTab = model.generatedSourceFilename(appSettings);
        }
    } catch (e) {
        console.warn('Settings load failed, using defaults:', e);
        appSettings = defaultAppSettings();
    }

    return appSettings;
}

/** Persist the active theme mode to the shared settings file. */
async function saveThemeMode(mode) {
    const normalized = normalizeThemeMode(mode);
    appSettings.appearance.theme = normalized;
    await invoke('set_theme_mode', { theme: normalized });
}

/** Remember the most recently loaded device/package for the next app launch. */
async function rememberLastUsedDevice(partNumber, package) {
    const part = String(partNumber || '').trim().toUpperCase();
    const pkg = String(package || '').trim();
    if (!part) return;

    appSettings.last_used.part_number = part;
    appSettings.last_used.package = pkg;
    await invoke('remember_last_used_device', {
        partNumber: part,
        package: pkg || null,
    });
}

/**
 * Resolve the device that should be loaded automatically on startup.
 * Returns null when the configured policy intentionally starts the app blank.
 */
function resolveStartupTarget(settings) {
    return model.resolveStartupTarget(settings);
}

// =============================================================================
// Pin Function Classification
// =============================================================================

/**
 * Classify a fixed (non-PPS) pin function into a group for the assignment dropdown.
 * Returns null for functions that should be hidden (e.g. raw RP numbers).
 * @param {string} name - Function name from the EDC data
 * @returns {string|null} Group name or null to skip
 */
function fixedFuncGroup(name) {
    if (/^OA\d/.test(name)) return 'Op-Amp';
    if (/^AN[A-Z]?\d+$|^AD\d+AN[A-Z]*\d+$/.test(name)) return 'ADC';
    if (/^CMP\d/.test(name)) return 'Comparator';
    if (/^IBIAS/.test(name)) return 'Bias';
    if (/^DAC/.test(name)) return 'DAC';
    if (/^OSCI|^CLKI/.test(name)) return 'Oscillator';
    if (/^OSCO|^CLKO/.test(name)) return 'Oscillator';
    if (/^INT0$/.test(name)) return 'Interrupt';
    if (/^PG[DC]\d/.test(name)) return 'Debug';
    if (/^TD[IO]$|^TMS$|^TCK$/.test(name)) return 'JTAG';
    if (/^A?SCL\d/.test(name)) return 'I2C';
    if (/^A?SDA\d/.test(name)) return 'I2C';
    if (/^PWM\d/.test(name)) return 'PWM';
    if (/^PCI\d/.test(name)) return 'PWM Fault/PCI';
    if (/^PWME[A-Z]/.test(name)) return 'PWM Event';
    if (/^PWMTRG/.test(name)) return 'PWM Trigger';
    if (/^OCM\d/.test(name)) return 'SCCP/MCCP';
    if (/^OCF[A-Z]/.test(name)) return 'SCCP/MCCP Fault';
    if (/^ICM\d/.test(name)) return 'Input Capture';
    if (/^T\d+CK$/.test(name)) return 'Timer';
    if (/^TCKI\d/.test(name)) return 'Timer';
    if (/^QE[AB]\d|^HOME\d|^INDX\d|^QEICCMP/.test(name)) return 'QEI';
    if (/^INT\d/.test(name)) return 'Interrupt';
    if (/^CLC\d+OUT/.test(name)) return 'CLC Output';
    if (/^CLCIN/.test(name)) return 'CLC Input';
    if (/^SENT/.test(name)) return 'SENT';
    if (/^C\d+(TX|RX)$/.test(name)) return 'CAN';
    if (/^U\d+/.test(name)) return 'UART';
    if (/^S[DC][OI]\d|^SS\d|^SCK\d/.test(name)) return 'SPI';
    if (/^REF[IO]$/.test(name)) return 'Reference Clock';
    if (/^ADCTRG/.test(name)) return 'ADC Trigger';
    if (/^PTGTRG/.test(name)) return 'PTG';
    if (/^RPV\d/.test(name)) return 'Virtual Pin';
    if (/^RP\d+$/.test(name)) return null;
    if (/^R[A-Z]\d+$/.test(name)) return 'GPIO';
    return 'Other';
}

/**
 * Infer the signal direction for a fixed (non-PPS) function.
 * @param {string} name - Function name
 * @returns {'in'|'out'|'io'}
 */
function fixedFuncDirection(name) {
    // Outputs
    if (/^OA\d+OUT/.test(name)) return 'out';
    if (/^DAC/.test(name)) return 'out';
    if (/^CLKO$|^OSCO$/.test(name)) return 'out';
    if (/^CMP\d+$/.test(name)) return 'out';
    if (/^CLC\d+OUT/.test(name)) return 'out';
    if (/^PWME[A-Z]/.test(name)) return 'out';
    if (/^PWM\d+[HL]$/.test(name)) return 'out';
    if (/^OCM\d/.test(name)) return 'out';
    if (/^PTGTRG/.test(name)) return 'out';
    if (/^QEICCMP/.test(name)) return 'out';
    if (/^U\d+TX$|^U\d+RTS$|^U\d+DTR$/.test(name)) return 'out';
    if (/^SDO\d/.test(name)) return 'out';
    if (/^SCK\d+OUT/.test(name)) return 'out';
    if (/^SS\d+OUT/.test(name)) return 'out';
    if (/^C\d+TX$/.test(name)) return 'out';
    if (/^SENT\d+OUT/.test(name)) return 'out';
    if (/^REFO$/.test(name)) return 'out';
    // Bidirectional
    if (/^R[A-Z]\d+$/.test(name)) return 'io';
    if (/^A?SCL\d|^A?SDA\d/.test(name)) return 'io';
    // Everything else defaults to input
    return 'in';
}

/**
 * Build the list of assignable peripherals for a given pin.
 * Includes both fixed functions (from pin hardware) and PPS remappable peripherals.
 * @param {Object} pin - Pin object from deviceData.pins
 * @returns {Array<{name:string, direction:string, ppsval:?number, group:string, fixed:boolean}>}
 */
function getAvailablePeripherals(pin) {
    const periphs = [];

    // Fixed (hardwired) functions from the pin's function list
    for (const fn of pin.functions) {
        if (isI2cRoutingFunction(fn) && !isI2cRoutingFunctionActive(fn)) continue;
        const group = fixedFuncGroup(fn);
        if (!group) continue;
        const dir = fixedFuncDirection(fn);
        periphs.push({
            name: fn,
            direction: dir,
            ppsval: null,
            group: group,
            fixed: true,
        });
    }

    // PPS remappable peripherals (only for pins with an RP number).
    // Skip any PPS peripheral that already exists as a fixed function on this pin —
    // the fixed entry is preferred because it needs no PPS register write.
    if (pin.rp_number !== null) {
        const fixedNames = new Set(periphs.map(p => p.name));

        for (const inp of deviceData.remappable_inputs) {
            if (fixedNames.has(inp.name)) continue;
            periphs.push({
                name: inp.name,
                direction: 'in',
                ppsval: null,
                group: periphGroupFine(inp.name),
                fixed: false,
            });
        }

        for (const out of deviceData.remappable_outputs) {
            if (fixedNames.has(out.name)) continue;
            periphs.push({
                name: out.name,
                direction: 'out',
                ppsval: out.ppsval,
                group: periphGroupFine(out.name),
                fixed: false,
            });
        }
    }

    // Sort: fixed functions first, then by group name, then by direction
    periphs.sort((a, b) => {
        if (a.fixed !== b.fixed) return a.fixed ? -1 : 1;
        const gc = a.group.localeCompare(b.group);
        if (gc !== 0) return gc;
        if (a.direction !== b.direction) return a.direction === 'out' ? -1 : 1;
        return a.name.localeCompare(b.name);
    });

    return periphs;
}
