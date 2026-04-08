/**
 * Tests for pure CLC state and register helpers.
 *
 * These guard the logic shared by the designer, schematic renderer, and config
 * persistence flow without requiring a DOM harness.
 */
const test = require('node:test');
const assert = require('node:assert/strict');

const clcModel = require('../static/app/05-clc-model.js');

test('default config builds normalized modules for the requested count', () => {
    const config = clcModel.createDefaultConfig(10);

    assert.equal(Object.keys(config).length, 10);
    assert.deepEqual(config[10], clcModel.defaultModule());
    assert.equal(clcModel.isModuleConfigured(config[10]), false);
});

test('default config falls back to the historical four-module baseline', () => {
    const config = clcModel.createDefaultConfig();

    assert.equal(Object.keys(config).length, clcModel.MODULE_COUNT);
    assert.deepEqual(config[1], clcModel.defaultModule());
    assert.equal(clcModel.isModuleConfigured(config[1]), false);
});

test('normalizeSavedConfig coerces sparse and partial modules into canonical shape', () => {
    const config = clcModel.normalizeSavedConfig({
        2: {
            ds: [1, 9],
            gates: [
                [1, 0, true],
            ],
            gpol: [0, 1],
            mode: 10,
            lcpol: 1,
            lcoe: false,
        },
        9: {
            mode: 1,
        },
    }, 10);

    assert.equal(config[2].ds[0], 1);
    assert.equal(config[2].ds[1], 1);
    assert.equal(config[2].ds[2], 0);
    assert.equal(config[2].mode, 2);
    assert.equal(config[2].gates[0][0], true);
    assert.equal(config[2].gates[0][1], false);
    assert.equal(config[2].gates[3][7], false);
    assert.equal(config[2].gpol[1], true);
    assert.equal(config[2].lcpol, true);
    assert.equal(config[2].lcoe, false);
    assert.equal(config[2].lcen, true);
    assert.equal(config[9].mode, 1);
});

test('computeRegisters packs CLC control and gate bits as expected', () => {
    const registers = clcModel.computeRegisters(clcModel.normalizeModule({
        ds: [1, 2, 3, 4],
        gates: [
            [true, false, false, false, false, false, false, false],
            [false, true, false, false, false, false, false, false],
            [false, false, true, false, false, false, false, false],
            [false, false, false, true, false, false, false, false],
        ],
        gpol: [true, false, true, false],
        mode: 5,
        lcpol: true,
        lcoe: true,
        lcen: true,
        intp: true,
        intn: false,
    }));

    assert.equal(registers.conl, 0x88A5);
    assert.equal(registers.conh, 0x0005);
    assert.equal(registers.sel, 0x4321);
    assert.equal(registers.glsl, 0x0201);
    assert.equal(registers.glsh, 0x0804);
    assert.equal(clcModel.hex16(registers.sel), '0x4321');
});

test('collectConfiguredModules omits default modules and preserves configured ones', () => {
    const config = clcModel.createDefaultConfig(10);
    config[3] = clcModel.normalizeModule({
        mode: 1,
    });
    config[8] = clcModel.normalizeModule({
        intp: true,
    });

    assert.deepEqual(clcModel.collectConfiguredModules(config), {
        3: config[3],
        8: config[8],
    });
});

test('resolveModuleCount prefers device inventory and falls back cleanly', () => {
    assert.equal(clcModel.resolveModuleCount({ device_info: { clc: 10 } }), 10);
    assert.equal(
        clcModel.resolveModuleCount({
            device_info: { clc: 0 },
            remappable_outputs: [{ name: 'CLC1OUT' }, { name: 'CLC10OUT' }],
        }),
        10
    );
    assert.equal(clcModel.resolveModuleCount({ device_info: { clc: 0 } }), clcModel.MODULE_COUNT);
    assert.equal(clcModel.resolveModuleCount(null), clcModel.MODULE_COUNT);
});

test('resolveSavedModuleCount keeps higher-indexed modules alive during config restore', () => {
    assert.equal(clcModel.resolveSavedModuleCount({ 10: { mode: 1 }, 3: { mode: 2 } }), 10);
    assert.equal(clcModel.resolveSavedModuleCount({ foo: {}, 0: {} }), 0);
});
