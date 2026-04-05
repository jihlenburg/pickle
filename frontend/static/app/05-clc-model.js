/**
 * Pure CLC state helpers and register packing.
 *
 * Keeps mode metadata, default module shapes, normalization, and register
 * calculation separate from the DOM-driven designer and SVG renderer.
 */
(function (root, factory) {
    const api = factory();
    if (typeof module !== 'undefined' && module.exports) {
        module.exports = api;
    }
    root.PickleClcModel = api;
}(typeof globalThis !== 'undefined' ? globalThis : this, function () {
    const MODULE_COUNT = 4;

    const MODES = Object.freeze([
        { value: 0, name: 'AND-OR',         code: '000', desc: 'Gate1·Gate2 + Gate3·Gate4' },
        { value: 1, name: 'OR-XOR',         code: '001', desc: '(G1+G2) XOR (G3+G4)' },
        { value: 2, name: '4-AND',          code: '010', desc: 'Gate1 · Gate2 · Gate3 · Gate4' },
        { value: 3, name: 'SR Latch',       code: '011', desc: 'S=(G1+G2), R=(G3+G4)' },
        { value: 4, name: 'D-FF + S/R',     code: '100', desc: 'D=G2, CLK=G1, S=G4, R=G3' },
        { value: 5, name: '2-in D-FF + R',  code: '101', desc: 'D=G2·G4, CLK=G1, R=G3' },
        { value: 6, name: 'JK-FF + R',      code: '110', desc: 'J=G2, CLK=G1, K=G4, R=G3' },
        { value: 7, name: 'Latch + S/R',    code: '111', desc: 'D=G2, LE=G1, S=G4, R=G3' },
    ]);

    function defaultModule() {
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

    function createDefaultConfig() {
        const config = {};
        for (let i = 1; i <= MODULE_COUNT; i++) {
            config[i] = defaultModule();
        }
        return config;
    }

    function normalizeGateBits(gates) {
        const defaults = defaultModule().gates;
        return defaults.map((defaultGate, gateIndex) => defaultGate.map((_, bitIndex) => (
            Boolean(gates?.[gateIndex]?.[bitIndex])
        )));
    }

    function normalizeModule(module) {
        const defaults = defaultModule();
        return {
            ds: defaults.ds.map((_, index) => Number.isFinite(module?.ds?.[index]) ? (module.ds[index] & 0x7) : 0),
            gates: normalizeGateBits(module?.gates),
            gpol: defaults.gpol.map((_, index) => Boolean(module?.gpol?.[index])),
            mode: Number.isFinite(module?.mode) ? (module.mode & 0x7) : 0,
            lcpol: Boolean(module?.lcpol),
            lcoe: module?.lcoe !== false,
            lcen: module?.lcen !== false,
            intp: Boolean(module?.intp),
            intn: Boolean(module?.intn),
        };
    }

    function normalizeSavedConfig(saved) {
        const config = createDefaultConfig();
        for (const [idx, module] of Object.entries(saved || {})) {
            const moduleIndex = parseInt(idx, 10);
            if (moduleIndex >= 1 && moduleIndex <= MODULE_COUNT) {
                config[moduleIndex] = normalizeModule(module);
            }
        }
        return config;
    }

    function isModuleConfigured(module) {
        if (!module) return false;
        if (module.mode !== 0) return true;
        for (let gateIndex = 0; gateIndex < 4; gateIndex++) {
            if (module.gpol[gateIndex]) return true;
            if (module.gates[gateIndex].some(Boolean)) return true;
        }
        if (module.ds.some((value) => value !== 0)) return true;
        return Boolean(module.lcpol || module.intp || module.intn);
    }

    function collectConfiguredModules(config) {
        const modules = {};
        for (let i = 1; i <= MODULE_COUNT; i++) {
            const module = config?.[i];
            if (isModuleConfigured(module)) {
                modules[i] = normalizeModule(module);
            }
        }
        return Object.keys(modules).length > 0 ? modules : null;
    }

    function computeRegisters(module) {
        if (!module) return { conl: 0, conh: 0, sel: 0, glsl: 0, glsh: 0 };

        let conl = (module.mode & 0x7);
        if (module.lcpol) conl |= (1 << 5);
        if (module.lcoe) conl |= (1 << 7);
        if (module.intn) conl |= (1 << 10);
        if (module.intp) conl |= (1 << 11);
        if (module.lcen) conl |= (1 << 15);

        let conh = 0;
        for (let g = 0; g < 4; g++) {
            if (module.gpol[g]) conh |= (1 << g);
        }

        const sel = (module.ds[0] & 0x7)
            | ((module.ds[1] & 0x7) << 4)
            | ((module.ds[2] & 0x7) << 8)
            | ((module.ds[3] & 0x7) << 12);

        let glsl = 0;
        let glsh = 0;
        for (let bit = 0; bit < 8; bit++) {
            if (module.gates[0][bit]) glsl |= (1 << bit);
            if (module.gates[1][bit]) glsl |= (1 << (bit + 8));
            if (module.gates[2][bit]) glsh |= (1 << bit);
            if (module.gates[3][bit]) glsh |= (1 << (bit + 8));
        }

        return { conl, conh, sel, glsl, glsh };
    }

    function hex16(value) {
        return '0x' + (value & 0xFFFF).toString(16).toUpperCase().padStart(4, '0');
    }

    return {
        MODULE_COUNT,
        MODES,
        defaultModule,
        createDefaultConfig,
        normalizeModule,
        normalizeSavedConfig,
        isModuleConfigured,
        collectConfiguredModules,
        computeRegisters,
        hex16,
    };
}));
