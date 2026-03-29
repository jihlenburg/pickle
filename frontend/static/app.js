/**
 * config-pic — Pin Configurator Frontend
 *
 * Single-page application for dsPIC33 pin multiplexing configuration.
 * Manages device data, pin assignments, code generation, and UI state.
 *
 * Architecture:
 *   - State: global variables (deviceData, assignments, signalNames)
 *   - Rendering: imperative DOM manipulation (no framework)
 *   - Backend: Tauri IPC via invoke()
 *   - File I/O: native Tauri dialogs handled by Rust commands
 *   - Undo: snapshot-based (structuredClone of state objects)
 */

const { invoke } = window.__TAURI__.core;

// =============================================================================
// State
// =============================================================================

/** @type {Object|null} Device data from invoke('load_device') */
let deviceData = null;

/** @type {Object.<number, {peripheral:string, direction:string, ppsval:?number, rp_number:?number, fixed:boolean}>} */
let assignments = {};

/** @type {Object.<number, string>} Pin position -> user-defined signal name */
let signalNames = {};

/** @type {Object.<string, string>} Generated file contents keyed by filename */
let generatedFiles = {};

/** @type {string} Currently visible code tab */
let activeTab = 'pin_config.c';

/** @type {{total:number, cached:number, available:boolean, ageHours:?number, isStale:boolean}} */
let indexCatalogState = {
    total: 0,
    cached: 0,
    available: false,
    ageHours: null,
    isStale: true,
};

// =============================================================================
// Undo / Redo
// =============================================================================

/** @type {Array<{assignments:Object, signalNames:Object}>} */
let undoStack = [];
/** @type {Array<{assignments:Object, signalNames:Object}>} */
let redoStack = [];
const MAX_UNDO = 50;

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
    renderDevice();
    checkConflicts();
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
    renderDevice();
    checkConflicts();
}

/** Global keyboard shortcut handler for undo (Cmd/Ctrl+Z) and redo (Cmd/Ctrl+Shift+Z or Cmd/Ctrl+Y). */
document.addEventListener('keydown', (e) => {
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
    if (/^CMP|^OA/.test(name)) return 'periph-cmp';
    return 'periph-other';
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
 *                          When switching packages (pkg provided), assignments are preserved.
 */
async function loadDevice(pkg) {
    const part = document.getElementById('part-input').value.trim().toUpperCase();
    if (!part) return;

    const isCached = cachedDevices.has(part);
    setStatus(isCached ? 'Loading...' : 'Downloading DFP pack...');

    try {
        deviceData = await invoke('load_device', { partNumber: part, package: pkg || null });

        // Populate package selector (only shown when multiple packages exist)
        const pkgSelect = document.getElementById('pkg-select');
        const pkgNames = Object.keys(deviceData.packages);
        if (pkgNames.length > 1) {
            pkgSelect.innerHTML = '';
            for (const name of pkgNames) {
                const opt = document.createElement('option');
                opt.value = name;
                opt.textContent = `${name} (${deviceData.packages[name].pin_count}p)`;
                if (name === deviceData.selected_package) opt.selected = true;
                pkgSelect.appendChild(opt);
            }
            pkgSelect.style.display = '';
        } else {
            pkgSelect.style.display = 'none';
        }

        // Show verify button when device is loaded
        document.getElementById('verify-btn').style.display = '';

        // Clear assignments only when switching parts, not packages
        if (!pkg) {
            assignments = {};
            undoStack = [];
            redoStack = [];
        }

        // Show configuration panels
        document.getElementById('save-btn').style.display = '';
        document.getElementById('load-btn-file').style.display = '';
        document.getElementById('osc-config').style.display = '';
        document.getElementById('fuse-config').style.display = '';

        renderDevice();
        setStatus(`${deviceData.part_number} — ${deviceData.selected_package}`);

        // Update cached device set if this was a new download
        if (!isCached) {
            cachedDevices.add(part);
            populateDeviceList();
        }
    } catch (e) {
        setStatus('Error: ' + (e.message || e));
    }
}

/** Update the status bar text in the header. */
function setStatus(msg) {
    const el = document.getElementById('status');
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
    if (/^SCL\d/.test(name)) return 'I2C';
    if (/^SDA\d/.test(name)) return 'I2C';
    if (/^PWM\d/.test(name)) return 'PWM';
    if (/^R[A-E]\d+$/.test(name)) return 'GPIO';
    if (/^RP\d+$/.test(name)) return null;
    return 'Other';
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
        const group = fixedFuncGroup(fn);
        if (!group) continue;
        let dir = 'in';
        if (/^OA\d+OUT|^DAC|^CLKO|^OSCO/.test(fn)) dir = 'out';
        if (/^R[A-E]\d+$/.test(fn)) dir = 'io';
        if (/^SCL\d|^SDA\d/.test(fn)) dir = 'io';
        periphs.push({
            name: fn,
            direction: dir,
            ppsval: null,
            group: group,
            fixed: true,
        });
    }

    // PPS remappable peripherals (only for pins with an RP number)
    if (pin.rp_number !== null) {
        for (const inp of deviceData.remappable_inputs) {
            periphs.push({
                name: inp.name,
                direction: 'in',
                ppsval: null,
                group: periphGroupFine(inp.name),
                fixed: false,
            });
        }

        for (const out of deviceData.remappable_outputs) {
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

// =============================================================================
// ICSP Pin Detection
// =============================================================================

/**
 * Return the currently selected ICSP pair number from the fuse UI (1, 2, or 3).
 * @returns {number}
 */
function getIcsPair() {
    const el = document.getElementById('fuse-ics');
    return el ? parseInt(el.value) : 1;
}

/**
 * Check if a pin is part of the active ICSP/debug interface.
 * Matches MCLR and the debug pair selected by the ICS fuse setting.
 * @param {Object} pin - Pin object with functions array
 * @returns {boolean}
 */
function isIcspPin(pin) {
    const pair = getIcsPair();
    return pin.functions.some(fn =>
        /^MCLR$/.test(fn) ||
        new RegExp(`^PGC${pair}$|^PGD${pair}$|^PGEC${pair}$|^PGED${pair}$`).test(fn)
    );
}

/**
 * Check if a function name belongs to the active ICSP debug pair.
 * @param {string} fn - Function name (e.g. "PGC1", "MCLR")
 * @returns {boolean}
 */
function isIcspFunction(fn) {
    const pair = getIcsPair();
    return /^MCLR$/.test(fn) ||
        new RegExp(`^PGC${pair}$|^PGD${pair}$|^PGEC${pair}$|^PGED${pair}$`).test(fn);
}

/**
 * Refresh ICSP gold highlighting on pin table rows, function tags,
 * and package diagram pins to match the current ICS fuse pair.
 */
function refreshIcspHighlight() {
    if (!deviceData) return;
    for (const pin of deviceData.pins) {
        const isIcsp = isIcspPin(pin);

        // Pin table row
        const row = document.getElementById(`pin-row-${pin.position}`);
        if (row) row.classList.toggle('icsp', isIcsp);

        // Package diagram pin
        const pkgPin = document.getElementById(`pkg-pin-${pin.position}`);
        if (pkgPin) pkgPin.classList.toggle('icsp', isIcsp);
    }
    // Function tags — check each individually
    document.querySelectorAll('.func-tag').forEach(span => {
        span.classList.toggle('icsp', isIcspFunction(span.textContent));
    });
}

// =============================================================================
// Conflict Detection
// =============================================================================

/**
 * Detect duplicate peripheral assignments and highlight conflicts on the
 * package diagram and pin table.
 * @returns {Set<number>} Set of conflicting pin positions
 */
function checkConflicts() {
    const box = document.getElementById('conflict-box');
    const conflicts = [];
    const conflictPins = new Set();

    // Build a map of peripheral+direction -> first assigned pin position
    const used = {};
    for (const [pos, assign] of Object.entries(assignments)) {
        const key = `${assign.peripheral}_${assign.direction}`;
        if (used[key]) {
            conflicts.push(
                `${assign.peripheral} (${assign.direction}) assigned to both pin ${used[key]} and pin ${pos}`
            );
            conflictPins.add(parseInt(used[key]));
            conflictPins.add(parseInt(pos));
        } else {
            used[key] = pos;
        }
    }

    box.textContent = conflicts.join('\n');

    // Apply/remove conflict highlighting on diagram and table elements
    if (deviceData) {
        for (const pin of deviceData.pins) {
            const pkgEl = document.getElementById(`pkg-pin-${pin.position}`);
            const rowEl = document.getElementById(`pin-row-${pin.position}`);
            const isConflict = conflictPins.has(pin.position);
            if (pkgEl) pkgEl.classList.toggle('conflict', isConflict);
            if (rowEl) rowEl.classList.toggle('conflict', isConflict);
        }
    }

    return conflictPins;
}

// =============================================================================
// Pin Table Rendering
// =============================================================================

/** Render the full pin table and package diagram from current deviceData and assignments. */
function renderDevice() {
    const tbody = document.getElementById('pin-tbody');
    tbody.innerHTML = '';

    const rpPins = deviceData.pins.filter(p => p.rp_number !== null);

    // Update summary bar
    document.getElementById('sum-pins').textContent = deviceData.pin_count;
    document.getElementById('sum-rp').textContent = rpPins.length;
    document.getElementById('sum-pkg').textContent = deviceData.selected_package;
    document.getElementById('summary').style.display = '';

    renderPackageDiagram();

    for (const pin of deviceData.pins) {
        const tr = document.createElement('tr');
        tr.className = 'pin-row';
        tr.id = `pin-row-${pin.position}`;
        if (pin.is_power) tr.classList.add('power');
        if (isIcspPin(pin)) tr.classList.add('icsp');

        // Column: pin number
        const tdNum = document.createElement('td');
        tdNum.className = 'pin-num';
        tdNum.textContent = pin.position;
        tr.appendChild(tdNum);

        // Column: pin name (port label or pad name)
        const tdName = document.createElement('td');
        tdName.className = 'pin-name';
        const portName = pin.port ? `R${pin.port}${pin.port_bit}` : pin.pad_name || pin.functions[0] || '—';
        tdName.textContent = portName;
        tr.appendChild(tdName);

        // Column: available functions (as colored tags)
        const tdFunc = document.createElement('td');
        const funcDiv = document.createElement('div');
        funcDiv.className = 'pin-functions';
        for (const fn of pin.functions) {
            const span = document.createElement('span');
            span.className = 'func-tag';
            if (/^RP\d+$/.test(fn)) span.classList.add('rp');
            if (/^AN[A-Z]?\d+$/.test(fn)) span.classList.add('analog');
            if (isIcspFunction(fn)) span.classList.add('icsp');
            span.textContent = fn;
            const desc = getDescription(fn);
            if (desc) span.title = desc;
            funcDiv.appendChild(span);
        }
        tdFunc.appendChild(funcDiv);
        tr.appendChild(tdFunc);

        // Column: assignment dropdown (non-power pins only)
        const tdAssign = document.createElement('td');
        if (!pin.is_power) {
            const periphs = getAvailablePeripherals(pin);

            const select = document.createElement('select');
            select.className = 'assign-select';
            select.dataset.pinPos = pin.position;
            select.dataset.rpNum = pin.rp_number ?? '';
            select.dataset.fixed = pin.rp_number === null ? '1' : '0';

            const optNone = document.createElement('option');
            optNone.value = '';
            optNone.textContent = '— unassigned —';
            select.appendChild(optNone);

            // Group peripherals into optgroups by type
            const seen = new Set();
            for (const p of periphs) {
                const key = p.group + (p.fixed ? '_fixed' : '_pps');
                if (seen.has(key)) continue;
                seen.add(key);

                const optgroup = document.createElement('optgroup');
                optgroup.label = p.fixed ? `${p.group} (fixed)` : p.group;
                const groupPeriphs = periphs.filter(x => x.group === p.group && x.fixed === p.fixed);
                for (const gp of groupPeriphs) {
                    const opt = document.createElement('option');
                    let label = gp.name;
                    if (!gp.fixed) {
                        const dirLabel = gp.direction === 'out' ? 'OUT' : 'IN';
                        label = `${gp.name} (${dirLabel})`;
                    } else if (gp.direction === 'io') {
                        label = `${gp.name} (IN/OUT)`;
                    }
                    opt.value = JSON.stringify({
                        name: gp.name, direction: gp.direction,
                        ppsval: gp.ppsval, fixed: gp.fixed,
                    });
                    opt.textContent = label;
                    opt.className = periphClass(gp.name);
                    const optDesc = getDescription(gp.name);
                    if (optDesc) opt.title = optDesc;
                    optgroup.appendChild(opt);
                }
                select.appendChild(optgroup);
            }

            // Restore previous assignment if present
            if (assignments[pin.position]) {
                const a = assignments[pin.position];
                const val = JSON.stringify({
                    name: a.peripheral, direction: a.direction,
                    ppsval: a.ppsval, fixed: a.fixed || false,
                });
                select.value = val;
            }

            select.addEventListener('change', onAssignChange);
            tdAssign.appendChild(select);
        }
        tr.appendChild(tdAssign);

        // Column: signal name input (non-power pins only)
        const tdSig = document.createElement('td');
        if (!pin.is_power) {
            const input = document.createElement('input');
            input.type = 'text';
            input.className = 'signal-input';
            input.placeholder = 'e.g. UART1_TX';
            input.dataset.pinPos = pin.position;
            if (signalNames[pin.position]) {
                input.value = signalNames[pin.position];
            }
            // Push undo snapshot when field gains focus (before user types)
            input.addEventListener('focus', () => pushUndo());
            input.addEventListener('input', (e) => {
                const pos = parseInt(e.target.dataset.pinPos);
                if (e.target.value.trim()) {
                    signalNames[pos] = e.target.value.trim();
                } else {
                    delete signalNames[pos];
                }
            });
            tdSig.appendChild(input);
        }
        tr.appendChild(tdSig);

        tbody.appendChild(tr);
    }

    updateSummary();
    checkConflicts();
}

// =============================================================================
// Package Diagram
// =============================================================================

/**
 * Get the display label for a pin in the package diagram.
 * Shows signal name or peripheral if assigned, otherwise port name.
 */
function pinLabel(pin) {
    const assign = assignments[pin.position];
    if (assign) {
        const sig = signalNames[pin.position];
        return sig || assign.peripheral;
    }
    return pin.port ? `R${pin.port}${pin.port_bit}` : pin.pad_name || pin.functions[0];
}

/** Check if the current package uses QFN/QFP layout (vs DIP/SSOP). */
function isQfnPackage() {
    return /QFN|QFP|TQFP/i.test(deviceData.selected_package);
}

/** Render the package diagram into the container, choosing DIP or QFN layout. */
function renderPackageDiagram() {
    const container = document.getElementById('pkg-container');
    container.innerHTML = '';

    if (isQfnPackage()) {
        renderQfnDiagram(container);
    } else {
        renderDipDiagram(container);
    }
}

/**
 * Create a single pin element for the package diagram.
 * @param {Object} pin - Pin data object
 * @param {string} side - Position: 'left', 'right', 'top', or 'bottom'
 * @returns {HTMLElement}
 */
function makePinEl(pin, side) {
    const el = document.createElement('div');
    el.className = `pkg-pin pkg-pin-${side}`;
    el.id = `pkg-pin-${pin.position}`;
    el.onclick = () => scrollToPin(pin.position);
    if (pin.is_power) el.classList.add('power');
    if (isIcspPin(pin)) el.classList.add('icsp');
    if (assignments[pin.position]) el.classList.add('assigned');

    const lbl = document.createElement('span');
    lbl.className = 'pkg-pin-label';
    const name = pinLabel(pin);

    if (side === 'left') {
        lbl.textContent = `${pin.position} ${name}`;
        el.appendChild(lbl);
    } else if (side === 'right') {
        lbl.textContent = `${name} ${pin.position}`;
        el.appendChild(lbl);
    } else if (side === 'top') {
        lbl.textContent = `${name}`;
        const num = document.createElement('span');
        num.className = 'pkg-pin-num';
        num.textContent = pin.position;
        el.appendChild(num);
        el.appendChild(lbl);
    } else {
        lbl.textContent = `${name}`;
        const num = document.createElement('span');
        num.className = 'pkg-pin-num';
        num.textContent = pin.position;
        el.appendChild(lbl);
        el.appendChild(num);
    }
    return el;
}

/** Render a DIP/SSOP-style package diagram (two rows of pins facing each other). */
function renderDipDiagram(container) {
    const pins = deviceData.pins;
    const half = Math.ceil(pins.length / 2);
    const leftPins = pins.slice(0, half);
    const rightPins = pins.slice(half).reverse();

    const diagram = document.createElement('div');
    diagram.className = 'pkg-diagram';

    const chip = document.createElement('div');
    chip.className = 'chip-body chip-dip';

    const notch = document.createElement('div');
    notch.className = 'notch';
    chip.appendChild(notch);

    const label = document.createElement('div');
    label.className = 'chip-label';
    label.textContent = `${deviceData.part_number}`;
    chip.appendChild(label);

    const sublabel = document.createElement('div');
    sublabel.className = 'chip-sublabel';
    sublabel.textContent = deviceData.selected_package;
    chip.appendChild(sublabel);

    const maxRows = Math.max(leftPins.length, rightPins.length);
    for (let i = 0; i < maxRows; i++) {
        const row = document.createElement('div');
        row.className = 'pin-row-diagram';
        if (leftPins[i]) row.appendChild(makePinEl(leftPins[i], 'left'));
        else row.appendChild(document.createElement('div'));
        if (rightPins[i]) row.appendChild(makePinEl(rightPins[i], 'right'));
        else row.appendChild(document.createElement('div'));
        chip.appendChild(row);
    }

    diagram.appendChild(chip);
    container.appendChild(diagram);
}

/** Render a QFN/QFP/TQFP-style package diagram (pins on all four sides). */
function renderQfnDiagram(container) {
    const pins = deviceData.pins;
    const n = pins.length;
    const side = Math.ceil(n / 4);

    const leftPins = pins.slice(0, side);
    const bottomPins = pins.slice(side, side * 2);
    const rightPins = pins.slice(side * 2, side * 3).reverse();
    const topPins = pins.slice(side * 3).reverse();

    const wrapper = document.createElement('div');
    wrapper.className = 'pkg-diagram';

    const grid = document.createElement('div');
    grid.className = 'qfn-grid';
    grid.style.gridTemplateColumns = `auto repeat(${bottomPins.length}, 1fr) auto`;
    grid.style.gridTemplateRows = `auto repeat(${leftPins.length}, 1fr) auto`;

    // Pin 1 marker (top-left corner)
    const marker = document.createElement('div');
    marker.className = 'qfn-corner qfn-marker';
    marker.innerHTML = '&#x25CF;';
    grid.appendChild(marker);

    // Top row pins
    for (const pin of topPins) {
        grid.appendChild(makePinEl(pin, 'top'));
    }
    for (let i = topPins.length; i < bottomPins.length; i++) {
        grid.appendChild(document.createElement('div'));
    }
    grid.appendChild(document.createElement('div'));

    // Middle rows: left pin | chip body | right pin
    for (let i = 0; i < leftPins.length; i++) {
        grid.appendChild(makePinEl(leftPins[i], 'left'));
        if (i === 0) {
            const body = document.createElement('div');
            body.className = 'qfn-body';
            body.style.gridColumn = `2 / ${bottomPins.length + 2}`;
            body.style.gridRow = `2 / ${leftPins.length + 2}`;
            body.innerHTML = `<div class="chip-label">${deviceData.part_number}</div>
                              <div class="chip-sublabel">${deviceData.selected_package}</div>`;
            grid.appendChild(body);
        }
        if (rightPins[i]) grid.appendChild(makePinEl(rightPins[i], 'right'));
        else grid.appendChild(document.createElement('div'));
    }

    // Bottom row
    grid.appendChild(document.createElement('div'));
    for (const pin of bottomPins) {
        grid.appendChild(makePinEl(pin, 'bottom'));
    }
    grid.appendChild(document.createElement('div'));

    wrapper.appendChild(grid);
    container.appendChild(wrapper);
}

/**
 * Scroll the pin table to center a specific pin row and briefly highlight it.
 * Uses getBoundingClientRect for reliable positioning in nested scroll containers.
 * @param {number} pos - Pin position number
 */
function scrollToPin(pos) {
    const row = document.getElementById(`pin-row-${pos}`);
    if (row) {
        const container = document.querySelector('.panel-left-scroll');
        if (container) {
            const containerRect = container.getBoundingClientRect();
            const rowRect = row.getBoundingClientRect();
            const offset = rowRect.top - containerRect.top + container.scrollTop;
            container.scrollTo({ top: offset - container.clientHeight / 2, behavior: 'smooth' });
        } else {
            row.scrollIntoView({ behavior: 'smooth', block: 'center' });
        }
        row.style.background = 'rgba(0,173,181,0.15)';
        setTimeout(() => { row.style.background = ''; }, 1500);
    }
}

// =============================================================================
// Assignment Change Handler
// =============================================================================

/** Handle peripheral assignment dropdown changes. Pushes undo state first. */
function onAssignChange(e) {
    pushUndo();

    const select = e.target;
    const pinPos = parseInt(select.dataset.pinPos);
    const rpNum = select.dataset.rpNum ? parseInt(select.dataset.rpNum) : null;
    const row = document.getElementById(`pin-row-${pinPos}`);

    if (select.value) {
        const { name, direction, ppsval, fixed } = JSON.parse(select.value);
        assignments[pinPos] = { peripheral: name, direction, ppsval, rp_number: rpNum, fixed: !!fixed };
        row.classList.add('assigned');
    } else {
        delete assignments[pinPos];
        row.classList.remove('assigned');
    }

    updateSummary();
    checkConflicts();
    renderPackageDiagram();
}

/** Update the "Assigned" count in the summary bar. */
function updateSummary() {
    document.getElementById('sum-assigned').textContent = Object.keys(assignments).length;
}

/** Sync assigned/unassigned state on existing diagram pin elements (without full re-render). */
function updateDiagramDots() {
    for (const pin of deviceData.pins) {
        const el = document.getElementById(`pkg-pin-${pin.position}`);
        if (el) {
            el.classList.toggle('assigned', !!assignments[pin.position]);
        }
    }
}

// =============================================================================
// Code Generation
// =============================================================================

/**
 * Request code generation from the backend and display the result.
 * Sends all current assignments, oscillator config, and fuse config.
 * Handles both multi-file (pin_config.h + .c) and single-file responses.
 */
async function generateCode() {
    if (!deviceData) return;

    const assignmentList = Object.entries(assignments).map(([pos, a]) => ({
        pin_position: parseInt(pos),
        rp_number: a.rp_number,
        peripheral: a.peripheral,
        direction: a.direction,
        ppsval: a.ppsval,
        fixed: a.fixed || false,
    }));

    if (assignmentList.length === 0) {
        document.getElementById('code-output').textContent = '// No pin assignments configured.';
        document.getElementById('code-tabs').style.display = 'none';
        return;
    }

    try {
        const payload = {
            part_number: deviceData.part_number,
            package: deviceData.selected_package,
            assignments: assignmentList,
            signal_names: signalNames,
            digital_pins: [],
            oscillator: getOscConfig(),
            fuses: getFuseConfig(),
        };
        const data = await invoke('generate_code', { request: payload });
        if (data.files) {
            generatedFiles = data.files;
            document.getElementById('code-tabs').style.display = '';
            showTab(activeTab);
        } else {
            generatedFiles = { 'pin_config.c': data.code };
            document.getElementById('code-tabs').style.display = 'none';
            document.getElementById('code-output').textContent = data.code;
        }
    } catch (e) {
        document.getElementById('code-output').textContent = '// Error generating code: ' + (e.message || e);
    }
}

/**
 * Switch the visible code output tab.
 * @param {string} tab - Filename to display (e.g. 'pin_config.c' or 'pin_config.h')
 */
function showTab(tab) {
    activeTab = tab;
    document.getElementById('code-output').textContent = generatedFiles[tab] || '';
    for (const btn of document.querySelectorAll('.code-tab')) {
        btn.classList.toggle('active', btn.dataset.file === tab);
    }
}

/** Copy the currently visible code output to the clipboard. */
async function copyCode() {
    const code = document.getElementById('code-output').textContent;
    try {
        await navigator.clipboard.writeText(code);
        const btn = document.getElementById('copy-btn');
        btn.textContent = 'Copied!';
        setStatus('Code copied to clipboard');
        setTimeout(() => { btn.textContent = 'Copy'; }, 1500);
    } catch (e) {
        setStatus('Error: clipboard access failed');
    }
}

/** Export all generated files (pin_config.c and pin_config.h) using a native folder picker. */
async function exportCode() {
    const files = Object.entries(generatedFiles);
    if (files.length === 0) return;

    try {
        const result = await invoke('export_generated_files_dialog', {
            request: {
                title: 'Export Generated C Files',
                files: generatedFiles,
            },
        });
        if (!result) return;

        const btn = document.getElementById('export-btn');
        btn.textContent = 'Exported!';
        setStatus(`Exported ${result.writtenFiles.length} files to ${result.directory}`);
        setTimeout(() => { btn.textContent = 'Export Files'; }, 1500);
    } catch (e) {
        setStatus('Error exporting files: ' + (e.message || e));
    }
}

/**
 * Export a formatted pin list as a plain text file for documentation.
 * Produces a clean table with pin number, name, assignment, and signal name.
 */
async function exportPinList() {
    if (!deviceData) return;

    const lines = [];
    const part = deviceData.part_number;
    const pkg = deviceData.selected_package;
    const date = new Date().toISOString().slice(0, 10);

    lines.push(`${part} — ${pkg}`);
    lines.push(`Pin Assignment List (${date})`);
    lines.push('');

    // Build rows and measure column widths
    const rows = [];
    for (const pin of deviceData.pins) {
        const num = String(pin.position);
        const name = pin.port ? `R${pin.port}${pin.port_bit}` : pin.pad_name || pin.functions[0] || '—';
        const assign = assignments[pin.position]
            ? assignments[pin.position].peripheral
            : (pin.is_power ? pin.functions[0] : '—');
        const sig = signalNames[pin.position] || '';
        rows.push([num, name, assign, sig]);
    }

    const headers = ['Pin', 'Name', 'Function', 'Signal'];
    const widths = headers.map((h, i) =>
        Math.max(h.length, ...rows.map(r => r[i].length))
    );

    const grid = document.getElementById('pinlist-grid')?.checked ?? true;

    const fmtRow = grid
        ? (cols) => cols.map((c, i) => ` ${c.padEnd(widths[i])} `).join('│')
        : (cols) => cols.map((c, i) => c.padEnd(widths[i])).join('  ');

    lines.push(fmtRow(headers));
    if (grid) {
        lines.push(widths.map(w => '─'.repeat(w + 2)).join('┼'));
    } else {
        lines.push(widths.map(w => '─'.repeat(w)).join('──'));
    }
    for (const row of rows) {
        lines.push(fmtRow(row));
    }

    lines.push('');
    lines.push(`Generated by config-pic`);

    const text = lines.join('\n');
    try {
        const result = await invoke('save_text_file_dialog', {
            request: {
                title: 'Save Pin Assignment List',
                suggestedName: `${part}_pinlist.txt`,
                contents: text,
                filters: [{ name: 'Text', extensions: ['txt'] }],
            },
        });
        if (result) {
            setStatus(`Saved pin list to ${result.path}`);
        }
    } catch (e) {
        setStatus('Error saving pin list: ' + (e.message || e));
    }
}

// =============================================================================
// Oscillator UI
// =============================================================================

/** Set up oscillator configuration UI — show/hide rows based on clock source selection. */
function setupOscUI() {
    const source = document.getElementById('osc-source');
    const crystalRow = document.getElementById('osc-crystal-row');
    const targetRow = document.getElementById('osc-target-row');
    const fcy = document.getElementById('osc-fcy-hint');
    const targetInput = document.getElementById('osc-target');

    source.addEventListener('change', () => {
        const val = source.value;
        const needsCrystal = val === 'pri' || val === 'pri_pll';
        const needsTarget = val === 'frc_pll' || val === 'pri_pll';
        crystalRow.style.display = needsCrystal ? '' : 'none';
        targetRow.style.display = needsTarget ? '' : 'none';
        updateFcyHint();
    });

    targetInput.addEventListener('input', updateFcyHint);

    function updateFcyHint() {
        const val = parseFloat(targetInput.value);
        if (val > 0) {
            fcy.textContent = `Fcy = ${(val / 2).toFixed(3)} MHz`;
        } else {
            fcy.textContent = '';
        }
    }
}

/** Read current oscillator UI values into a config object for the backend. */
function getOscConfig() {
    const source = document.getElementById('osc-source').value;
    if (!source) return null;
    return {
        source: source,
        target_fosc_mhz: parseFloat(document.getElementById('osc-target').value) || 0,
        crystal_mhz: parseFloat(document.getElementById('osc-crystal').value) || 0,
        poscmd: document.getElementById('osc-poscmd').value,
    };
}

// =============================================================================
// Configuration Fuse UI
// =============================================================================

/** Set up fuse configuration UI — conditionally show/hide dependent fields. */
function setupFuseUI() {
    // Show WDT prescaler only when watchdog is not OFF
    const fwdten = document.getElementById('fuse-fwdten');
    const wdtpsRow = document.getElementById('fuse-wdtps').closest('.fuse-row');
    fwdten.addEventListener('change', () => {
        wdtpsRow.style.display = fwdten.value === 'OFF' ? 'none' : '';
    });
    wdtpsRow.style.display = fwdten.value === 'OFF' ? 'none' : '';

    // Show BOR voltage only when brown-out reset is enabled
    const boren = document.getElementById('fuse-boren');
    const borvRow = document.getElementById('fuse-borv').closest('.fuse-row');
    boren.addEventListener('change', () => {
        borvRow.style.display = boren.value === 'OFF' ? 'none' : '';
    });

    // Update ICSP highlighting when debug pair changes
    const ics = document.getElementById('fuse-ics');
    ics.addEventListener('change', () => refreshIcspHighlight());
}

/** Read current fuse UI values into a config object for the backend. */
function getFuseConfig() {
    return {
        ics: parseInt(document.getElementById('fuse-ics').value),
        jtagen: document.getElementById('fuse-jtagen').value,
        fwdten: document.getElementById('fuse-fwdten').value,
        wdtps: document.getElementById('fuse-wdtps').value,
        boren: document.getElementById('fuse-boren').value,
        borv: document.getElementById('fuse-borv').value,
    };
}

// =============================================================================
// Save / Load Configuration
// =============================================================================

/** Save the current configuration (assignments, signals, osc, fuses) via a native file dialog. */
async function saveConfig() {
    if (!deviceData) return;
    const config = {
        part_number: deviceData.part_number,
        package: deviceData.selected_package,
        assignments: assignments,
        signal_names: signalNames,
        oscillator: getOscConfig(),
        fuses: getFuseConfig(),
    };
    try {
        const result = await invoke('save_text_file_dialog', {
            request: {
                title: 'Save Pin Configuration',
                suggestedName: `${deviceData.part_number}_${deviceData.selected_package}.json`,
                contents: JSON.stringify(config, null, 2),
                filters: [{ name: 'JSON', extensions: ['json'] }],
            },
        });
        if (result) {
            setStatus(`Saved config to ${result.path}`);
        }
    } catch (e) {
        setStatus('Error saving config: ' + (e.message || e));
    }
}

/**
 * Open a previously saved configuration JSON file via a native file picker.
 */
async function openConfigDialog() {
    try {
        const result = await invoke('open_text_file_dialog', {
            request: {
                title: 'Open Pin Configuration',
                filters: [{ name: 'JSON', extensions: ['json'] }],
            },
        });
        if (!result) return;
        await loadConfigText(result.contents, result.path);
    } catch (e) {
        setStatus('Error loading config: ' + (e.message || e));
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
            setStatus('Invalid config file: missing part_number');
            return;
        }

        document.getElementById('part-input').value = config.part_number;

        // Restore assignments and signal names before loading device
        assignments = config.assignments || {};
        signalNames = config.signal_names || {};

        // Convert string keys (from JSON) back to integer keys
        const intAssign = {};
        for (const [k, v] of Object.entries(assignments)) {
            intAssign[parseInt(k)] = v;
        }
        assignments = intAssign;

        const intSig = {};
        for (const [k, v] of Object.entries(signalNames)) {
            intSig[parseInt(k)] = v;
        }
        signalNames = intSig;

        // Restore oscillator settings
        if (config.oscillator) {
            document.getElementById('osc-source').value = config.oscillator.source || '';
            document.getElementById('osc-target').value = config.oscillator.target_fosc_mhz || 200;
            document.getElementById('osc-crystal').value = config.oscillator.crystal_mhz || 8;
            document.getElementById('osc-poscmd').value = config.oscillator.poscmd || 'EC';
            document.getElementById('osc-source').dispatchEvent(new Event('change'));
        }

        // Restore fuse settings
        if (config.fuses) {
            document.getElementById('fuse-ics').value = config.fuses.ics || 1;
            document.getElementById('fuse-jtagen').value = config.fuses.jtagen || 'OFF';
            document.getElementById('fuse-fwdten').value = config.fuses.fwdten || 'OFF';
            document.getElementById('fuse-wdtps').value = config.fuses.wdtps || 'PS1024';
            document.getElementById('fuse-boren').value = config.fuses.boren || 'ON';
            document.getElementById('fuse-borv').value = config.fuses.borv || 'BOR_HIGH';
            document.getElementById('fuse-fwdten').dispatchEvent(new Event('change'));
            document.getElementById('fuse-boren').dispatchEvent(new Event('change'));
        }

        await loadDevice(config.package || null);
        const sourceName = sourcePath ? ` from ${sourcePath.split(/[\\/]/).pop()}` : '';
        setStatus(`Loaded config${sourceName}: ${config.part_number} — ${config.package || 'default'}`);
    } catch (e) {
        setStatus('Error loading config: ' + (e.message || e));
    }
}

// =============================================================================
// XC16 Compile Check
// =============================================================================

/** @type {boolean} Whether the XC16 compiler was detected on the server */
let compilerAvailable = false;

/** Check if XC16 compiler is available and show the "Check with XC16" button if so. */
async function checkCompiler() {
    try {
        const data = await invoke('compiler_info');
        compilerAvailable = data.available;
        if (compilerAvailable) {
            document.getElementById('check-btn').style.display = '';
            document.getElementById('check-btn').title = data.version || 'XC16';
        }
    } catch (e) {
        /* compiler check is optional */
    }
}

/** Send the currently displayed code to the backend for XC16 compilation check. */
async function compileCheck() {
    if (!deviceData) return;
    const code = document.getElementById('code-output').textContent;
    if (!code || code.startsWith('//') || code.startsWith('Load a device')) return;

    const resultBox = document.getElementById('compile-result');
    resultBox.className = 'compile-result';
    resultBox.textContent = 'Compiling...';

    try {
        const data = await invoke('compile_check', { request: { code: generatedFiles['pin_config.c'] || code, header: generatedFiles['pin_config.h'] || '', partNumber: deviceData.part_number } });
        if (data.success && !data.warnings) {
            resultBox.className = 'compile-result success';
            resultBox.textContent = 'XC16: compiled successfully — no errors or warnings';
        } else if (data.success && data.warnings) {
            resultBox.className = 'compile-result warning';
            resultBox.textContent = 'XC16: compiled with warnings:\n' + data.warnings;
        } else {
            resultBox.className = 'compile-result error';
            resultBox.textContent = 'XC16: compilation failed:\n' + data.errors;
        }
    } catch (e) {
        resultBox.className = 'compile-result error';
        resultBox.textContent = 'Error: ' + (e.message || e);
    }
}

// =============================================================================
// Event Listeners & Initialization
// =============================================================================

document.getElementById('load-btn').addEventListener('click', () => loadDevice());
document.getElementById('gen-btn').addEventListener('click', generateCode);
document.getElementById('check-btn').addEventListener('click', compileCheck);
document.getElementById('copy-btn').addEventListener('click', copyCode);
document.getElementById('export-btn').addEventListener('click', exportCode);
document.getElementById('verify-btn').addEventListener('click', verifyPinout);
document.getElementById('pinlist-btn').addEventListener('click', exportPinList);
document.getElementById('save-btn').addEventListener('click', saveConfig);
document.getElementById('load-btn-file').addEventListener('click', openConfigDialog);
document.getElementById('part-input').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') loadDevice();
});
document.getElementById('pkg-select').addEventListener('change', (e) => {
    loadDevice(e.target.value);
});

// Code tab switching
for (const btn of document.querySelectorAll('.code-tab')) {
    btn.addEventListener('click', () => showTab(btn.dataset.file));
}

// Populate device list for combo box autocomplete
/** @type {Set<string>} Devices available locally (no download needed) */
let cachedDevices = new Set();

function populateDeviceList() {
    invoke('list_devices').then(data => {
        const dl = document.getElementById('device-list');
        dl.innerHTML = '';
        cachedDevices = new Set(data.cached || []);
        (data.devices || []).forEach(d => {
            const opt = document.createElement('option');
            opt.value = d;
            dl.appendChild(opt);
        });
        updateIndexBadge(data.total, data.cached_count);
        refreshIndexStatus();
    }).catch(e => console.warn('Device list fetch failed:', e));
}

function updateIndexBadge(total, cached) {
    const badge = document.getElementById('index-badge');
    if (!badge) return;
    if (typeof total === 'number') indexCatalogState.total = total;
    if (typeof cached === 'number') indexCatalogState.cached = cached;

    if (indexCatalogState.total > 0) {
        const freshness = indexCatalogState.available
            ? (indexCatalogState.isStale ? 'stale' : 'fresh')
            : 'offline';
        const ageText = typeof indexCatalogState.ageHours === 'number'
            ? `${indexCatalogState.ageHours.toFixed(1)}h old`
            : 'age unknown';

        badge.textContent = `${indexCatalogState.total} devices | ${indexCatalogState.cached} cached | ${freshness}`;
        badge.title = indexCatalogState.available
            ? `Device catalog is ${freshness}. ${ageText}. Click to refresh from Microchip.`
            : 'Device catalog not yet available. Click to fetch from Microchip.';
        badge.dataset.stale = String(indexCatalogState.isStale);
        badge.dataset.available = String(indexCatalogState.available);
        badge.style.display = '';
    } else {
        badge.style.display = 'none';
    }
}

async function refreshIndexStatus() {
    try {
        const data = await invoke('index_status');
        indexCatalogState.available = !!data.available;
        indexCatalogState.ageHours = data.age_hours;
        indexCatalogState.isStale = !!data.is_stale;
        updateIndexBadge();
    } catch {
        indexCatalogState.available = false;
        indexCatalogState.ageHours = null;
        indexCatalogState.isStale = true;
        updateIndexBadge();
    }
}

async function refreshIndex() {
    const badge = document.getElementById('index-badge');
    if (badge) badge.textContent = 'Refreshing...';
    setStatus('Refreshing device catalog...');
    try {
        const data = await invoke('refresh_index');
        if (data.success) {
            indexCatalogState.available = true;
            indexCatalogState.ageHours = data.age_hours;
            indexCatalogState.isStale = false;
            populateDeviceList();
            setStatus(`Catalog refreshed: ${data.device_count} devices across ${data.pack_count} packs`);
        } else {
            if (badge) badge.textContent = 'Refresh failed';
            setStatus('Error: failed to refresh device catalog');
        }
    } catch (e) {
        if (badge) badge.textContent = 'Refresh failed';
        setStatus('Error: failed to refresh device catalog');
    }
}

populateDeviceList();

// =============================================================================
// Theme Toggle
// =============================================================================

/** Resolve the effective theme ('dark' or 'light') for a given mode. */
function resolveTheme(mode) {
    if (mode === 'system') {
        return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
    }
    return mode;
}

/** Label for the theme toggle button. */
function themeLabel(mode) {
    if (mode === 'dark') return 'Dark';
    if (mode === 'light') return 'Light';
    return 'System';
}

/** Initialize theme from localStorage and wire toggle button. */
function setupTheme() {
    const saved = localStorage.getItem('config-pic-theme') || 'dark';
    document.documentElement.setAttribute('data-theme', resolveTheme(saved));
    const btn = document.getElementById('theme-toggle');
    btn.textContent = themeLabel(saved);

    const cycle = { dark: 'light', light: 'system', system: 'dark' };
    let current = saved;

    btn.addEventListener('click', () => {
        current = cycle[current] || 'dark';
        document.documentElement.setAttribute('data-theme', resolveTheme(current));
        localStorage.setItem('config-pic-theme', current);
        btn.textContent = themeLabel(current);
    });

    // When in system mode, follow OS changes in real time.
    window.matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => {
        if (localStorage.getItem('config-pic-theme') === 'system') {
            document.documentElement.setAttribute('data-theme', resolveTheme('system'));
        }
    });
}

// =============================================================================
// Pinout Verification (Anthropic API)
// =============================================================================

/** @type {Object|null} Last verification result */
let verifyResult = null;

/** Check if API key is configured. */
async function checkApiKey() {
    try {
        const data = await invoke('api_key_status');
        const btn = document.getElementById('verify-btn');
        if (btn) {
            btn.title = data.configured
                ? `API key configured (${data.hint})`
                : 'No API key — configure in .env';
        }
        return data.configured;
    } catch { return false; }
}

/** Trigger pinout verification with a PDF upload. */
async function verifyPinout() {
    if (!deviceData) {
        setStatus('Load a device first');
        return;
    }

    // Check API key
    await checkApiKey();

    try {
        const file = await invoke('open_binary_file_dialog', {
            request: {
                title: 'Select Datasheet PDF',
                filters: [{ name: 'PDF', extensions: ['pdf'] }],
            },
        });
        if (!file) return;

        const panel = document.getElementById('verify-panel');
        const output = document.getElementById('verify-output');
        panel.style.display = '';
        output.innerHTML = `<div class="verify-loading">Analyzing ${escapeHtml(file.name)} with Anthropic... this may take 30–60 seconds.</div>`;
        setStatus(`Verifying pinout from ${file.name}...`);

        const storedKey = localStorage.getItem('config-pic-api-key');
        const data = await invoke('verify_pinout', {
            pdfBase64: file.base64,
            partNumber: deviceData.part_number,
            package: deviceData.selected_package || null,
            apiKey: storedKey || null,
        });
        verifyResult = data;
        renderVerifyResult(verifyResult);
        setStatus('Verification complete');
    } catch (e) {
        document.getElementById('verify-output').innerHTML = `<div class="verify-error">Error: ${e.message || e}</div>`;
        setStatus('Verification error');
    }
}

/** Render the verification diff result. */
function renderVerifyResult(result) {
    const output = document.getElementById('verify-output');
    if (!result || !result.packages || Object.keys(result.packages).length === 0) {
        output.innerHTML = '<div class="verify-error">No package data found in datasheet.</div>';
        return;
    }

    let html = '';

    // Notes
    if (result.notes && result.notes.length) {
        html += '<div class="verify-notes">';
        result.notes.forEach(n => { html += `<div class="verify-note">${escapeHtml(n)}</div>`; });
        html += '</div>';
    }

    // Package tabs
    const pkgNames = Object.keys(result.packages);
    html += '<div class="verify-pkg-tabs">';
    pkgNames.forEach((name, i) => {
        const pkg = result.packages[name];
        const corrCount = (pkg.corrections || []).length;
        const scoreClass = pkg.match_score >= 0.95 ? 'score-good' : pkg.match_score >= 0.8 ? 'score-warn' : 'score-bad';
        const badge = corrCount > 0 ? ` <span class="verify-corr-badge">${corrCount}</span>` : '';
        html += `<button class="verify-pkg-tab${i === 0 ? ' active' : ''}" data-pkg="${name}">`;
        html += `${escapeHtml(name)} (${pkg.pin_count}p)`;
        html += ` <span class="verify-score ${scoreClass}">${Math.round(pkg.match_score * 100)}%</span>`;
        html += `${badge}</button>`;
    });
    html += '</div>';

    // Package details (one div per package)
    pkgNames.forEach((name, i) => {
        const pkg = result.packages[name];
        html += `<div class="verify-pkg-detail${i === 0 ? '' : ' hidden'}" data-pkg="${name}">`;

        // Corrections section
        if (pkg.corrections && pkg.corrections.length > 0) {
            html += '<div class="verify-corrections">';
            html += `<h4>Corrections (${pkg.corrections.length})</h4>`;
            pkg.corrections.forEach(c => {
                const typeLabel = {
                    'wrong_pad': 'Wrong Pad',
                    'missing_functions': 'Missing Functions',
                    'extra_functions': 'Extra Functions',
                    'missing_pin': 'Missing Pin',
                    'extra_pin': 'Extra Pin',
                }[c.correction_type] || c.correction_type;
                html += `<div class="verify-corr-item">`;
                html += `<span class="verify-corr-type">${typeLabel}</span> `;
                html += `Pin ${c.pin_position}: `;
                if (c.current_pad) html += `<span class="verify-old">${escapeHtml(c.current_pad)}</span>`;
                if (c.current_pad && c.datasheet_pad) html += ' → ';
                if (c.datasheet_pad) html += `<span class="verify-new">${escapeHtml(c.datasheet_pad)}</span>`;
                if (c.note) html += ` <span class="verify-corr-note">${escapeHtml(c.note)}</span>`;
                html += `</div>`;
            });
            html += '</div>';
        } else {
            html += '<div class="verify-match">All pins match current data.</div>';
        }

        // Full pin table
        html += '<table class="verify-table"><thead><tr>';
        html += '<th>Pin</th><th>Datasheet</th><th>Current</th><th>Status</th>';
        html += '</tr></thead><tbody>';

        const currentPins = {};
        if (deviceData && deviceData.pins) {
            deviceData.pins.forEach(p => { currentPins[p.position] = p; });
        }

        const sortedPositions = Object.keys(pkg.pins).map(Number).sort((a, b) => a - b);
        for (const pos of sortedPositions) {
            const dsPad = pkg.pins[pos];
            const cur = currentPins[pos];
            const curPad = cur ? (cur.pad_name || cur.pad) : '—';
            const match = cur && normalizePad(dsPad) === normalizePad(curPad);
            const statusClass = match ? 'verify-ok' : cur ? 'verify-diff' : 'verify-new';
            const statusText = match ? '✓' : cur ? '≠' : 'NEW';

            html += `<tr class="${statusClass}">`;
            html += `<td>${pos}</td>`;
            html += `<td>${escapeHtml(dsPad)}</td>`;
            html += `<td>${escapeHtml(curPad)}</td>`;
            html += `<td>${statusText}</td>`;
            html += `</tr>`;
        }
        html += '</tbody></table>';

        // Apply button for this package
        html += `<button class="verify-apply-btn" data-pkg="${name}">Apply "${escapeHtml(name)}" as Overlay</button>`;
        html += '</div>';
    });

    output.innerHTML = html;

    // Wire package tab switching
    output.querySelectorAll('.verify-pkg-tab').forEach(tab => {
        tab.addEventListener('click', () => {
            output.querySelectorAll('.verify-pkg-tab').forEach(t => t.classList.remove('active'));
            tab.classList.add('active');
            output.querySelectorAll('.verify-pkg-detail').forEach(d => d.classList.add('hidden'));
            output.querySelector(`.verify-pkg-detail[data-pkg="${tab.dataset.pkg}"]`)?.classList.remove('hidden');
        });
    });

    // Wire apply buttons
    output.querySelectorAll('.verify-apply-btn').forEach(btn => {
        btn.addEventListener('click', () => applyVerifiedOverlay(btn.dataset.pkg));
    });
}

/** Normalize pad name for comparison (strip trailing _N suffixes). */
function normalizePad(name) {
    return (name || '').toUpperCase().replace(/_\d+$/, '');
}

/** Escape HTML special characters. */
function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

/** Apply a verified package overlay to the device. */
async function applyVerifiedOverlay(pkgName) {
    if (!verifyResult || !verifyResult.packages[pkgName]) return;

    const pkg = verifyResult.packages[pkgName];
    const payload = {
        part_number: verifyResult.part_number,
        packages: {
            [pkgName]: {
                pin_count: pkg.pin_count,
                pins: pkg.pins,
                pin_functions: pkg.pin_functions || {},
            }
        }
    };

    try {
        const data = await invoke('apply_overlay', { request: payload });
        if (data.success) {
            setStatus(`Overlay saved for ${pkgName}. Reloading...`);
            // Reload device to pick up new overlay
            await loadDevice(pkgName);
        } else {
            setStatus('Failed to save overlay');
        }
    } catch (e) {
        setStatus('Error saving overlay: ' + (e.message || e));
    }
}

// Menu bar event listener (events emitted from Rust menu handlers)
if (window.__TAURI__?.event?.listen) {
    window.__TAURI__.event.listen('menu-action', (event) => {
        switch (event.payload) {
            case 'open':
                openConfigDialog();
                break;
            case 'save':
                saveConfig();
                break;
            case 'export':
                exportCode();
                break;
            case 'undo':
                undo();
                break;
            case 'redo':
                redo();
                break;
            case 'generate':
                generateCode();
                break;
            case 'copy_code':
                copyCode();
                break;
        }
    });
}

// Initialize UI and auto-load default device
setupTheme();
checkCompiler();
checkApiKey();
setupOscUI();
setupFuseUI();
if (document.getElementById('part-input').value) {
    loadDevice();
}
