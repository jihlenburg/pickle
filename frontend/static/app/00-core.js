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

/** @type {{appearance:{theme:string}, startup:{device:string, package:string}, toolchain:{fallback_compiler:string, family_compilers:{pic24:string, dspic33:string}}, codegen:{output_basename:string}, verification:{provider:string}, last_used:{part_number:string, package:string}}} */
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
    if (/^AN|^ADC/.test(name)) return 'periph-adc';
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
    if (/^AN[A-Z]?\d+$|^VREF[+-]?$|^IBIAS/.test(name)) return 'tag-adc';
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
    setStatus(isCached ? appConfig.ui.deviceLoad.cachedStatus : appConfig.ui.deviceLoad.remoteStatus);

    try {
        deviceData = await invoke('load_device', { partNumber: part, package: pkg || null });

        // Populate package selector. Keep it visible when a synthetic fallback
        // package was hidden so the user can still see which real package is active.
        const pkgSelect = $('pkg-select');
        const allPkgNames = Object.keys(deviceData.packages);
        const pkgNames = visiblePackageNames();
        const syntheticHidden = allPkgNames.some((name) => isSyntheticPackage(name) && !pkgNames.includes(name));
        if (pkgNames.length > 1 || syntheticHidden) {
            pkgSelect.innerHTML = '';
            for (const name of pkgNames) {
                const meta = deviceData.packages[name];
                const opt = document.createElement('option');
                opt.value = name;
                opt.textContent = `${displayPackageName(name, { meta })} (${meta.pin_count}p)`;
                if (isSyntheticPackage(name, meta)) {
                    opt.title = 'Fallback package from the EDC. This is not a real package name; verify against the datasheet to import the actual package.';
                }
                if (name === deviceData.selected_package) opt.selected = true;
                pkgSelect.appendChild(opt);
            }
            pkgSelect.disabled = pkgNames.length <= 1;
            pkgSelect.title = syntheticHidden
                ? 'The synthetic EDC fallback package was hidden. This is the real datasheet-backed package currently in use.'
                : isSyntheticPackage(deviceData.selected_package)
                    ? 'Current package is a fallback from the EDC and not a real package name. Verify against the datasheet to import the actual package.'
                    : '';
            showElement(pkgSelect);
        } else {
            pkgSelect.disabled = false;
            hideElement(pkgSelect);
        }

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

        // Show the view toggle once a device is loaded
        showElement('view-toggle');
        await checkCompiler(deviceData.part_number);

        renderCurrentEditorView();
        syncConfigDocumentAfterDeviceLoad({ preserveState, markDirty: options.markDirty });
        if (isSyntheticPackage(deviceData.selected_package)) {
            setStatus(`${deviceData.part_number} — ${displayPackageName(deviceData.selected_package, { long: true })}. Verify against the datasheet to import the actual package.`);
        } else {
            setStatus(`${deviceData.part_number} — ${deviceData.selected_package}`);
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
        setStatus('Error: ' + (e.message || e));
    }
}

/** Update the bottom status bar text. */
function setStatus(msg) {
    const el = $('status');
    if (!el) return;
    const text = String(msg || '').trim();
    const lower = text.toLowerCase();
    let tone = 'info';

    if (!text) tone = 'info';
    else if (lower.includes('error') || lower.includes('failed') || lower.includes('not found')) tone = 'error';
    else if (lower.includes('loading') || lower.includes('downloading') || lower.includes('refreshing') || lower.includes('verifying') || lower.includes('compiling') || lower.includes('analyzing')) tone = 'busy';
    else if (lower.includes('loaded') || lower.includes('saved') || lower.includes('export') || lower.includes('complete') || lower.includes('success')) tone = 'success';

    el.textContent = text || 'Ready';
    el.dataset.tone = tone;
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

function displayPackageName(name, options = {}) {
    const meta = options.meta || packageMeta(name);
    if (!name) {
        return '—';
    }
    if (!isSyntheticPackage(name, meta)) {
        return name;
    }
    return options.long
        ? `${name} [not a real package]`
        : `${name} [not real]`;
}

function visiblePackageNames() {
    if (!deviceData?.packages) {
        return [];
    }

    const pkgNames = Object.keys(deviceData.packages);
    const realPinCounts = new Set(
        pkgNames
            .filter((name) => !isSyntheticPackage(name))
            .map((name) => deviceData.packages[name]?.pin_count)
            .filter((pinCount) => typeof pinCount === 'number')
    );

    return pkgNames.filter((name) => {
        const meta = deviceData.packages[name];
        if (!isSyntheticPackage(name, meta)) {
            return true;
        }
        return !realPinCounts.has(meta?.pin_count);
    });
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
    if (/^AN[A-Z]?\d+$/.test(name)) return 'ADC';
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
