/**
 * CLC designer state and rendering.
 *
 * Keeps the dense logic-cell editor isolated from unrelated workflow code so
 * save/load and code generation can treat it as one maintained subsystem.
 */
const clcModel = window.PickleClcModel;

// =============================================================================
// CLC Designer
// =============================================================================

/**
 * CLC configuration state — one entry per module (1-indexed).
 * Each module maps directly to its CLCx register set.
 *
 * ds[0-3]: Data Selection MUX values (DS1-DS4), 0-7 each
 * gates[0-3]: Array of 8 booleans per gate — [D1T, D1N, D2T, D2N, D3T, D3N, D4T, D4N]
 * gpol[0-3]: Gate polarity inversion bits (G1POL-G4POL)
 * mode: Logic function mode (0-7)
 * lcpol: Output polarity inversion
 * lcoe: Output enable to pin
 * lcen: Module enable
 * intp: Interrupt on positive edge
 * intn: Interrupt on negative edge
 */
let clcConfig = {};
let clcActiveModule = 1;
const CLC_MODES = clcModel.MODES;

function deviceHasClc() {
    return !!deviceData?.has_clc;
}

function getClcModuleCount() {
    return clcModel.resolveModuleCount(deviceData);
}

function getSavedClcModuleCount(saved) {
    return clcModel.resolveSavedModuleCount(saved);
}

function syncClcDesignerState() {
    const moduleCount = getClcModuleCount();
    clcConfig = clcModel.normalizeSavedConfig(clcConfig, moduleCount);
    clcActiveModule = Math.min(Math.max(clcActiveModule, 1), moduleCount);
}

function clcEmptyMessage() {
    if (!deviceData) {
        return 'Load a device to configure CLC modules.';
    }
    if (!deviceHasClc()) {
        return 'This device has no CLC peripheral. The CLC editor and datasheet CLC lookup are disabled for this part.';
    }
    return 'CLC input sources are not available yet. Verify the datasheet to import them if needed.';
}

function updateClcTabState() {
    const tab = document.querySelector('.right-tab[data-tab="clc"]');
    if (!tab) return;

    const disabled = !!deviceData && !deviceHasClc();
    tab.disabled = disabled;
    tab.classList.toggle('is-disabled', disabled);
    tab.title = disabled
        ? 'This device has no CLC peripheral.'
        : 'Configure CLC modules';

    if (disabled && tab.classList.contains('active') && typeof switchRightTab === 'function') {
        switchRightTab('info');
    }
}

/** Initialize CLC config state for all modules. */
function initClcConfig() {
    clcConfig = clcModel.createDefaultConfig(getClcModuleCount());
    clcActiveModule = 1;
}

/** Check if a CLC module has been configured (differs from default). */
function isClcModuleConfigured(idx) {
    return clcModel.isModuleConfigured(clcConfig[idx]);
}

/**
 * Compute the 5 register values for a CLC module.
 * @returns {{ conl: number, conh: number, sel: number, glsl: number, glsh: number }}
 */
function computeClcRegisters(idx) {
    return clcModel.computeRegisters(clcConfig[idx]);
}

/** Format a 16-bit value as 0x#### hex string. */
function hex16(val) {
    return clcModel.hex16(val);
}

/** Render the full CLC designer panel for the active module. */
function renderClcDesigner() {
    const designer = document.getElementById('clc-designer');
    const empty = document.getElementById('clc-empty');
    if (!designer || !empty) return;

    if (!deviceData || !deviceHasClc()) {
        designer.style.display = 'none';
        empty.style.display = '';
        empty.textContent = clcEmptyMessage();
        return;
    }

    designer.style.display = '';
    empty.style.display = 'none';
    empty.textContent = clcEmptyMessage();
    syncClcDesignerState();

    renderClcModuleTabs();
    renderClcInputs();
    renderClcGateMatrix();
    renderClcModeCards();
    renderClcOutputControls();
    updateClcRegisters();
    if (typeof renderClcSchematic === 'function') renderClcSchematic();
}

/** Render the available CLC module selector tabs for the loaded device. */
function renderClcModuleTabs() {
    const container = document.getElementById('clc-module-tabs');
    container.innerHTML = '';

    for (let i = 1; i <= getClcModuleCount(); i++) {
        const btn = document.createElement('button');
        btn.className = 'clc-module-tab';
        if (i === clcActiveModule) btn.classList.add('active');
        btn.textContent = `CLC${i}`;
        if (isClcModuleConfigured(i)) {
            const dot = document.createElement('span');
            dot.className = 'clc-tab-dot';
            btn.appendChild(dot);
        }
        btn.addEventListener('click', () => {
            clcActiveModule = i;
            renderClcDesigner();
        });
        container.appendChild(btn);
    }
}

/** Render the 4 input source dropdowns (DS1-DS4). */
function renderClcInputs() {
    const container = document.getElementById('clc-inputs');
    container.innerHTML = '';
    const mod = clcConfig[clcActiveModule];

    for (let d = 0; d < 4; d++) {
        const label = document.createElement('label');
        label.textContent = `Data ${d + 1}`;
        container.appendChild(label);

        const select = document.createElement('select');
        select.dataset.ds = d;

        for (let s = 0; s < 8; s++) {
            const opt = document.createElement('option');
            opt.value = s;
            opt.textContent = getClcSourceLabel(d, s);
            select.appendChild(opt);
        }
        select.value = mod.ds[d];

        select.addEventListener('change', (e) => {
            applyClcMutation(() => {
                clcConfig[clcActiveModule].ds[parseInt(e.target.dataset.ds, 10)] = parseInt(e.target.value, 10);
            });
        });
        container.appendChild(select);
    }
}

/**
 * Get a human-readable label for a CLC input source.
 * DS index (0-3) selects which group of 8 CLCIN signals;
 * source (0-7) selects within that group.
 *
 * The actual mapping is device-specific. This provides reasonable
 * defaults for dsPIC33CK and generic labels for unknown devices.
 */
function getClcSourceLabel(dsIndex, source) {
    if (deviceData && deviceData.clc_input_sources) {
        const group = deviceData.clc_input_sources[dsIndex];
        if (group && group[source]) {
            return `${source}: ${group[source]}`;
        }
    }

    const clcinIdx = dsIndex * 8 + source;
    return `${source}: CLCIN${clcinIdx}`;
}

/** Render the 4x4 data gate matrix with T/N checkboxes and polarity toggles. */
function renderClcGateMatrix() {
    const container = document.getElementById('clc-gate-matrix');
    container.innerHTML = '';
    const mod = clcConfig[clcActiveModule];

    const table = document.createElement('table');
    const thead = document.createElement('thead');
    const hrow = document.createElement('tr');
    hrow.innerHTML = '<th class="clc-gate-hdr"></th>';
    for (let d = 1; d <= 4; d++) {
        hrow.innerHTML += `<th colspan="2">Data ${d}</th>`;
    }
    hrow.innerHTML += '<th>Polarity</th>';
    thead.appendChild(hrow);

    const subrow = document.createElement('tr');
    subrow.innerHTML = '<th class="clc-gate-hdr">Gate</th>';
    for (let d = 0; d < 4; d++) {
        subrow.innerHTML += '<th>T</th><th>N</th>';
    }
    subrow.innerHTML += '<th></th>';
    thead.appendChild(subrow);
    table.appendChild(thead);

    const tbody = document.createElement('tbody');
    for (let g = 0; g < 4; g++) {
        const tr = document.createElement('tr');

        const tdLabel = document.createElement('td');
        tdLabel.className = 'clc-gate-label';
        tdLabel.textContent = `Gate ${g + 1}`;
        tr.appendChild(tdLabel);

        for (let d = 0; d < 4; d++) {
            const tIdx = d * 2;
            const nIdx = d * 2 + 1;

            for (const [bitIdx, label] of [[tIdx, 'T'], [nIdx, 'N']]) {
                const td = document.createElement('td');
                const cb = document.createElement('input');
                cb.type = 'checkbox';
                cb.checked = mod.gates[g][bitIdx];
                cb.dataset.gate = g;
                cb.dataset.bit = bitIdx;
                cb.title = `Gate ${g + 1} Data ${d + 1} ${label === 'T' ? 'True' : 'Negated'}`;
                cb.addEventListener('change', (e) => {
                    applyClcMutation(() => {
                        const gi = parseInt(e.target.dataset.gate, 10);
                        const bi = parseInt(e.target.dataset.bit, 10);
                        clcConfig[clcActiveModule].gates[gi][bi] = e.target.checked;
                    });
                });
                td.appendChild(cb);
                tr.appendChild(td);
            }
        }

        const tdPol = document.createElement('td');
        const polBtn = document.createElement('button');
        polBtn.className = 'clc-pol-toggle';
        if (mod.gpol[g]) polBtn.classList.add('active');
        polBtn.textContent = mod.gpol[g] ? 'INV' : 'NOR';
        polBtn.title = `Gate ${g + 1} polarity: ${mod.gpol[g] ? 'Inverted' : 'Normal'}`;
        polBtn.dataset.gate = g;
        polBtn.addEventListener('click', (e) => {
            applyClcMutation(() => {
                const gi = parseInt(e.currentTarget.dataset.gate, 10);
                clcConfig[clcActiveModule].gpol[gi] = !clcConfig[clcActiveModule].gpol[gi];
                renderClcGateMatrix();
            });
        });
        tdPol.appendChild(polBtn);
        tr.appendChild(tdPol);

        tbody.appendChild(tr);
    }
    table.appendChild(tbody);
    container.appendChild(table);
}

/** Render the 8 logic function mode selection cards. */
function renderClcModeCards() {
    const container = document.getElementById('clc-mode-cards');
    container.innerHTML = '';
    const mod = clcConfig[clcActiveModule];

    for (const mode of CLC_MODES) {
        const card = document.createElement('div');
        card.className = 'clc-mode-card';
        if (mod.mode === mode.value) card.classList.add('selected');

        const name = document.createElement('div');
        name.className = 'clc-mode-name';
        name.textContent = mode.name;
        card.appendChild(name);

        const code = document.createElement('div');
        code.className = 'clc-mode-code';
        code.textContent = mode.code;
        card.appendChild(code);

        card.title = mode.desc;
        card.addEventListener('click', () => {
            applyClcMutation(() => {
                clcConfig[clcActiveModule].mode = mode.value;
                renderClcModeCards();
            });
        });
        container.appendChild(card);
    }
}

/** Render the output control checkboxes (LCOE, LCPOL, LCEN, INTP, INTN). */
function renderClcOutputControls() {
    const container = document.getElementById('clc-output-controls');
    container.innerHTML = '';
    const mod = clcConfig[clcActiveModule];

    const controls = [
        { key: 'lcen', label: 'Enable (LCEN)', title: 'Enable CLC module' },
        { key: 'lcoe', label: 'Output pin (LCOE)', title: 'Enable CLC output to I/O pin' },
        { key: 'lcpol', label: 'Invert out (LCPOL)', title: 'Invert the logic output' },
        { key: 'intp', label: 'IRQ rising (INTP)', title: 'Interrupt on positive edge of output' },
        { key: 'intn', label: 'IRQ falling (INTN)', title: 'Interrupt on negative edge of output' },
    ];

    for (const ctrl of controls) {
        const label = document.createElement('label');
        label.title = ctrl.title;
        const cb = document.createElement('input');
        cb.type = 'checkbox';
        cb.checked = mod[ctrl.key];
        cb.addEventListener('change', (e) => {
            applyClcMutation(() => {
                clcConfig[clcActiveModule][ctrl.key] = e.target.checked;
            });
        });
        label.appendChild(cb);
        label.appendChild(document.createTextNode(ctrl.label));
        container.appendChild(label);
    }
}

/** Update the register value display for the active CLC module. */
function updateClcRegisters() {
    const container = document.getElementById('clc-registers');
    container.innerHTML = '';
    const regs = computeClcRegisters(clcActiveModule);
    const n = clcActiveModule;

    const regList = [
        { name: `CLC${n}CONL`, value: regs.conl },
        { name: `CLC${n}CONH`, value: regs.conh },
        { name: `CLC${n}SEL`, value: regs.sel },
        { name: `CLC${n}GLSL`, value: regs.glsl },
        { name: `CLC${n}GLSH`, value: regs.glsh },
    ];

    for (const reg of regList) {
        const item = document.createElement('div');
        item.className = 'clc-reg-item';

        const nameSpan = document.createElement('span');
        nameSpan.className = 'clc-reg-name';
        nameSpan.textContent = reg.name;
        item.appendChild(nameSpan);

        const valSpan = document.createElement('span');
        valSpan.className = 'clc-reg-value';
        valSpan.textContent = hex16(reg.value);
        valSpan.title = 'Click to copy';
        valSpan.addEventListener('click', () => {
            navigator.clipboard.writeText(hex16(reg.value));
            setStatus(`Copied ${reg.name} = ${hex16(reg.value)}`);
        });
        item.appendChild(valSpan);

        container.appendChild(item);
    }
}

/** Collect the CLC configuration for save/codegen. Returns null if no modules configured. */
function getClcConfig() {
    return clcModel.collectConfiguredModules(clcConfig, getClcModuleCount());
}

/** Re-apply saved CLC configuration after device load. */
function applyClcConfig(saved) {
    const moduleCount = Math.max(getClcModuleCount(), getSavedClcModuleCount(saved));
    clcConfig = clcModel.normalizeSavedConfig(saved, moduleCount);
    clcActiveModule = 1;
}
