/**
 * CLC designer state and rendering.
 *
 * Keeps the dense logic-cell editor isolated from unrelated workflow code so
 * save/load and code generation can treat it as one maintained subsystem.
 */

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
const CLC_MODULE_COUNT = 4;

/** Logic function mode definitions (MODE<2:0>) */
const CLC_MODES = [
    { value: 0, name: 'AND-OR',         code: '000', desc: 'Gate1·Gate2 + Gate3·Gate4' },
    { value: 1, name: 'OR-XOR',         code: '001', desc: '(G1+G2) XOR (G3+G4)' },
    { value: 2, name: '4-AND',          code: '010', desc: 'Gate1 · Gate2 · Gate3 · Gate4' },
    { value: 3, name: 'SR Latch',       code: '011', desc: 'S=(G1+G2), R=(G3+G4)' },
    { value: 4, name: 'D-FF + S/R',     code: '100', desc: 'D=G2, CLK=G1, S=G4, R=G3' },
    { value: 5, name: '2-in D-FF + R',  code: '101', desc: 'D=G2·G4, CLK=G1, R=G3' },
    { value: 6, name: 'JK-FF + R',      code: '110', desc: 'J=G2, CLK=G1, K=G4, R=G3' },
    { value: 7, name: 'Latch + S/R',    code: '111', desc: 'D=G2, LE=G1, S=G4, R=G3' },
];

/** Default (reset) state for one CLC module. */
function defaultClcModule() {
    return {
        ds: [0, 0, 0, 0],
        gates: [
            [false, false, false, false, false, false, false, false],
            [false, false, false, false, false, false, false, false],
            [false, false, false, false, false, false, false, false],
            [false, false, false, false, false, false, false, false],
        ],
        gpol: [false, false, false, false],
        mode: 0,
        lcpol: false,
        lcoe: true,
        lcen: true,
        intp: false,
        intn: false,
    };
}

/** Initialize CLC config state for all modules. */
function initClcConfig() {
    clcConfig = {};
    for (let i = 1; i <= CLC_MODULE_COUNT; i++) {
        clcConfig[i] = defaultClcModule();
    }
    clcActiveModule = 1;
}

/** Check if a CLC module has been configured (differs from default). */
function isClcModuleConfigured(idx) {
    const mod = clcConfig[idx];
    if (!mod) return false;
    if (mod.mode !== 0) return true;
    for (let g = 0; g < 4; g++) {
        if (mod.gpol[g]) return true;
        if (mod.gates[g].some(Boolean)) return true;
    }
    if (mod.ds.some(v => v !== 0)) return true;
    if (mod.lcpol || mod.intp || mod.intn) return true;
    return false;
}

/**
 * Compute the 5 register values for a CLC module.
 * @returns {{ conl: number, conh: number, sel: number, glsl: number, glsh: number }}
 */
function computeClcRegisters(idx) {
    const mod = clcConfig[idx];
    if (!mod) return { conl: 0, conh: 0, sel: 0, glsl: 0, glsh: 0 };

    // CLCxCONL: bit15=LCEN, bit11=INTP, bit10=INTN, bit7=LCOE, bit5=LCPOL, bits2-0=MODE
    let conl = (mod.mode & 0x7);
    if (mod.lcpol) conl |= (1 << 5);
    if (mod.lcoe) conl |= (1 << 7);
    if (mod.intn) conl |= (1 << 10);
    if (mod.intp) conl |= (1 << 11);
    if (mod.lcen) conl |= (1 << 15);

    // CLCxCONH: bit3=G4POL, bit2=G3POL, bit1=G2POL, bit0=G1POL
    let conh = 0;
    for (let g = 0; g < 4; g++) {
        if (mod.gpol[g]) conh |= (1 << g);
    }

    // CLCxSEL: bits14-12=DS4, bits10-8=DS3, bits6-4=DS2, bits2-0=DS1
    const sel = (mod.ds[0] & 0x7)
              | ((mod.ds[1] & 0x7) << 4)
              | ((mod.ds[2] & 0x7) << 8)
              | ((mod.ds[3] & 0x7) << 12);

    // CLCxGLSL: bits15-8=Gate2 enables, bits7-0=Gate1 enables
    // Per gate byte: bit7=D4T, bit6=D4N, bit5=D3T, bit4=D3N, bit3=D2T, bit2=D2N, bit1=D1T, bit0=D1N
    let glsl = 0;
    for (let b = 0; b < 8; b++) {
        if (mod.gates[0][b]) glsl |= (1 << b);
        if (mod.gates[1][b]) glsl |= (1 << (b + 8));
    }

    // CLCxGLSH: bits15-8=Gate4, bits7-0=Gate3
    let glsh = 0;
    for (let b = 0; b < 8; b++) {
        if (mod.gates[2][b]) glsh |= (1 << b);
        if (mod.gates[3][b]) glsh |= (1 << (b + 8));
    }

    return { conl, conh, sel, glsl, glsh };
}

/** Format a 16-bit value as 0x#### hex string. */
function hex16(val) {
    return '0x' + (val & 0xFFFF).toString(16).toUpperCase().padStart(4, '0');
}

/** Render the full CLC designer panel for the active module. */
function renderClcDesigner() {
    if (!deviceData) return;

    document.getElementById('clc-designer').style.display = '';
    document.getElementById('clc-empty').style.display = 'none';

    renderClcModuleTabs();
    renderClcInputs();
    renderClcGateMatrix();
    renderClcModeCards();
    renderClcOutputControls();
    updateClcRegisters();
    if (typeof renderClcSchematic === 'function') renderClcSchematic();
}

/** Render the CLC1-4 module selector tabs. */
function renderClcModuleTabs() {
    const container = document.getElementById('clc-module-tabs');
    container.innerHTML = '';

    for (let i = 1; i <= CLC_MODULE_COUNT; i++) {
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
            clcConfig[clcActiveModule].ds[parseInt(e.target.dataset.ds, 10)] = parseInt(e.target.value, 10);
            updateClcRegisters();
            renderClcModuleTabs();
            if (typeof renderClcSchematic === 'function') renderClcSchematic();
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
                    const gi = parseInt(e.target.dataset.gate, 10);
                    const bi = parseInt(e.target.dataset.bit, 10);
                    clcConfig[clcActiveModule].gates[gi][bi] = e.target.checked;
                    updateClcRegisters();
                    renderClcModuleTabs();
                    if (typeof renderClcSchematic === 'function') renderClcSchematic();
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
            const gi = parseInt(e.currentTarget.dataset.gate, 10);
            clcConfig[clcActiveModule].gpol[gi] = !clcConfig[clcActiveModule].gpol[gi];
            renderClcGateMatrix();
            updateClcRegisters();
            renderClcModuleTabs();
            if (typeof renderClcSchematic === 'function') renderClcSchematic();
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
            clcConfig[clcActiveModule].mode = mode.value;
            renderClcModeCards();
            updateClcRegisters();
            renderClcModuleTabs();
            if (typeof renderClcSchematic === 'function') renderClcSchematic();
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
            clcConfig[clcActiveModule][ctrl.key] = e.target.checked;
            updateClcRegisters();
            renderClcModuleTabs();
            if (typeof renderClcSchematic === 'function') renderClcSchematic();
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
    const modules = {};
    let anyConfigured = false;
    for (let i = 1; i <= CLC_MODULE_COUNT; i++) {
        if (isClcModuleConfigured(i)) {
            modules[i] = clcConfig[i];
            anyConfigured = true;
        }
    }
    return anyConfigured ? modules : null;
}

/** Re-apply saved CLC configuration after device load. */
function applyClcConfig(saved) {
    initClcConfig();
    if (!saved) return;
    for (const [idx, mod] of Object.entries(saved)) {
        const i = parseInt(idx, 10);
        if (i >= 1 && i <= CLC_MODULE_COUNT && mod) {
            clcConfig[i] = {
                ds: mod.ds || [0, 0, 0, 0],
                gates: mod.gates || defaultClcModule().gates,
                gpol: mod.gpol || [false, false, false, false],
                mode: mod.mode ?? 0,
                lcpol: !!mod.lcpol,
                lcoe: mod.lcoe !== false,
                lcen: mod.lcen !== false,
                intp: !!mod.intp,
                intn: !!mod.intn,
            };
        }
    }
}
