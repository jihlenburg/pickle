/**
 * Smoke tests for the pure frontend model helpers.
 *
 * This suite intentionally avoids DOM dependencies so regressions in
 * assignment normalization or startup/settings logic fail fast in CI.
 */
const test = require('node:test');
const assert = require('node:assert/strict');

const config = require('../static/app/config.js');
const model = require('../static/app/model.js');

test('default settings use dark theme and last-used startup policy', () => {
    assert.deepEqual(model.defaultAppSettings(), {
        appearance: { theme: config.defaults.themeMode },
        startup: { device: 'last-used', package: '' },
        toolchain: {
            fallback_compiler: config.defaults.toolchain.fallbackCompiler,
            family_compilers: {
                pic24: config.defaults.toolchain.familyCompilers.pic24,
                dspic33: config.defaults.toolchain.familyCompilers.dspic33,
            },
        },
        codegen: {
            output_basename: config.defaults.codegen.outputBasename,
        },
        last_used: { part_number: '', package: '' },
    });
});

test('theme normalization only allows dark, light, or system', () => {
    assert.equal(model.normalizeThemeMode('light'), 'light');
    assert.equal(model.normalizeThemeMode('system'), 'system');
    assert.equal(model.normalizeThemeMode('dark'), 'dark');
    assert.equal(model.normalizeThemeMode(' SYSTEM '), 'system');
    assert.equal(model.normalizeThemeMode(' Light '), 'light');
    assert.equal(model.normalizeThemeMode(undefined), 'dark');
    assert.equal(model.normalizeThemeMode('weird'), 'dark');
});

test('startup target prefers last used device unless fixed device is configured', () => {
    assert.deepEqual(model.resolveStartupTarget({
        startup: { device: 'last-used', package: '' },
        last_used: { part_number: 'dspic33ck64mp102', package: 'TQFP-28' },
    }), {
        partNumber: 'DSPIC33CK64MP102',
        package: 'TQFP-28',
    });

    assert.deepEqual(model.resolveStartupTarget({
        startup: { device: 'dspic33ck128mp206', package: 'QFN-28' },
        last_used: { part_number: 'DSPIC33CK64MP102', package: 'SSOP-28' },
    }), {
        partNumber: 'DSPIC33CK128MP206',
        package: 'QFN-28',
    });

    assert.deepEqual(model.resolveStartupTarget({
        startup: { device: ' LAST-USED ', package: ' ignored ' },
        last_used: { part_number: ' dspic33ch128mp508 ', package: ' TQFP-48 ' },
    }), {
        partNumber: 'DSPIC33CH128MP508',
        package: 'TQFP-48',
    });

    assert.equal(model.resolveStartupTarget({
        startup: { device: 'last-used', package: '' },
        last_used: { part_number: '', package: '' },
    }), null);
});

test('compiler resolution follows PIC24 and dsPIC33 family defaults', () => {
    const settings = model.defaultAppSettings();

    assert.equal(model.detectDeviceFamily('PIC24FJ128GA204'), 'pic24');
    assert.equal(model.detectDeviceFamily('DSPIC33CK64MP102'), 'dspic33');
    assert.equal(model.detectDeviceFamily('PIC18F27Q43'), 'unknown');

    assert.equal(model.resolveCompilerCommand(settings, 'PIC24FJ128GA204'), 'xc16-gcc');
    assert.equal(model.resolveCompilerCommand(settings, 'DSPIC33CK64MP102'), 'xc-dsc-gcc');
    assert.equal(model.resolveCompilerCommand(settings, 'PIC18F27Q43'), 'xc-dsc-gcc');
});

test('codegen basename normalization yields stable source and header filenames', () => {
    assert.equal(model.normalizeOutputBasename(' MCU Init '), 'mcu_init');
    assert.equal(model.normalizeOutputBasename('...'), 'mcu_init');
    assert.equal(model.generatedSourceFilename({ codegen: { output_basename: 'board_setup' } }), 'board_setup.c');
    assert.equal(model.generatedHeaderFilename({ codegen: { output_basename: 'board_setup' } }), 'board_setup.h');
});

test('generated file helpers prefer actual files and keep source before header', () => {
    const files = {
        'board_setup.h': 'header',
        'board_setup.c': 'source',
        'board_setup.map': 'map',
    };

    assert.deepEqual(model.sortGeneratedFilenames(files), [
        'board_setup.c',
        'board_setup.h',
        'board_setup.map',
    ]);
    assert.equal(model.resolveGeneratedSourceFile(files, null), 'board_setup.c');
    assert.equal(model.resolveGeneratedHeaderFile(files, null), 'board_setup.h');
});

test('oscillator-managed fuse fields track the selected clock source', () => {
    assert.deepEqual(
        model.oscillatorManagedFuseFields('frc_pll', 'EC'),
        ['FNOSC', 'IESO', 'POSCMD', 'FCKSM', 'PLLKEN'],
    );
    assert.deepEqual(
        model.oscillatorManagedFuseFields('pri', 'XT'),
        ['FNOSC', 'IESO', 'POSCMD', 'FCKSM', 'XTCFG'],
    );
    assert.deepEqual(model.oscillatorManagedFuseFields('', 'EC'), []);
});

test('analog helpers classify inputs, outputs, and shared analog functions', () => {
    assert.equal(model.isAnalogInput('AN7'), true);
    assert.equal(model.isAnalogInput('CMP1D'), true);
    assert.equal(model.isAnalogInput('OA2IN+'), true);
    assert.equal(model.isAnalogOutput('OA1OUT'), true);
    assert.equal(model.isAnalogOutput('DACOUT'), true);
    assert.equal(model.isAnalogFunction('DAC1OUT'), true);
    assert.equal(model.isAnalogFunction('U1TX'), false);
});

test('assignment helpers normalize single and multi-assignment forms', () => {
    const assignments = {
        5: { peripheral: 'U1TX', direction: 'out', ppsval: 1, rp_number: 36, fixed: false },
        6: [
            { peripheral: 'AN0', direction: 'in', ppsval: null, rp_number: null, fixed: true },
            { peripheral: 'CMP1A', direction: 'in', ppsval: null, rp_number: null, fixed: true },
        ],
    };

    assert.deepEqual(model.getAssignmentsAt(assignments, 5), [assignments[5]]);
    assert.deepEqual(model.getAssignmentsAt(assignments, 6), assignments[6]);
    assert.deepEqual(model.getAssignmentsAt(assignments, 99), []);
    assert.equal(model.hasAssignmentFor(assignments, 6, 'CMP1A'), true);
    assert.equal(model.hasAssignmentFor(assignments, 5, 'U2TX'), false);
    assert.equal(model.primaryAssignment(assignments, 6).peripheral, 'AN0');
    assert.equal(model.primaryAssignment(assignments, 99), null);
});

test('assignment iteration, flattening, and reverse lookup stay deterministic', () => {
    const assignments = {
        5: { peripheral: 'U1TX', direction: 'out', ppsval: 1, rp_number: 36 },
        6: [
            { peripheral: 'AN0', direction: 'in', ppsval: null, rp_number: null, fixed: true },
            { peripheral: 'CMP1A', direction: 'in', ppsval: null, rp_number: null, fixed: true },
        ],
    };

    const visited = [];
    model.forEachAssignedPin(assignments, (pinPos, entries) => {
        visited.push([pinPos, entries.map(entry => entry.peripheral)]);
    });

    assert.deepEqual(visited, [
        [5, ['U1TX']],
        [6, ['AN0', 'CMP1A']],
    ]);

    assert.deepEqual(model.flattenAssignments(assignments), [
        { pinPosition: 5, rpNumber: 36, peripheral: 'U1TX', direction: 'out', ppsval: 1, fixed: false },
        { pinPosition: 6, rpNumber: null, peripheral: 'AN0', direction: 'in', ppsval: null, fixed: true },
        { pinPosition: 6, rpNumber: null, peripheral: 'CMP1A', direction: 'in', ppsval: null, fixed: true },
    ]);

    assert.deepEqual(model.buildReverseAssignments(assignments), {
        U1TX: 5,
        AN0: 6,
        CMP1A: 6,
    });
});

test('position maps normalize string keys back into numeric positions', () => {
    assert.deepEqual(model.normalizePositionMap({
        '1': { peripheral: 'U1RX' },
        '12': 'SIG',
        bad: 'skip me',
    }), {
        1: { peripheral: 'U1RX' },
        12: 'SIG',
    });
});

test('empty assignment maps stay stable across helper calls', () => {
    const assignments = {};

    assert.deepEqual(model.flattenAssignments(assignments), []);
    assert.deepEqual(model.buildReverseAssignments(assignments), {});

    const visited = [];
    model.forEachAssignedPin(assignments, (pinPos) => visited.push(pinPos));
    assert.deepEqual(visited, []);
});
