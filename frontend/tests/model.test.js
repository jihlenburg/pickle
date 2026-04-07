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
        verification: {
            provider: 'auto',
        },
        onboarding: {
            welcome_intro_seen: false,
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

test('package-label normalization hides internal EDC package codes', () => {
    assert.equal(model.normalizeFriendlyPackageName('STX04 (48-pin uQFN)'), '48-PIN VQFN');
    assert.equal(model.normalizeFriendlyPackageName('48-PIN TQFP'), '48-PIN TQFP');
    assert.equal(
        model.normalizeFriendlyPackageName('48-PIN VQFN/TQFP (dsPIC33AKXXXMPS505/dsPIC33AKXXXMPS205)'),
        '48-PIN VQFN/TQFP',
    );
    assert.equal(model.normalizeFriendlyPackageName(''), '—');
});

test('package visibility prefers overlay-backed entries over equivalent built-in packages', () => {
    const packages = {
        'STX32 (48-pin VQFN)': {
            pin_count: 48,
            source: 'edc',
        },
        '48-PIN VQFN': {
            pin_count: 48,
            source: 'overlay',
        },
        '48-PIN TQFP': {
            pin_count: 48,
            source: 'overlay',
        },
    };

    assert.deepEqual(model.visiblePackageNames(packages), [
        '48-PIN VQFN',
        '48-PIN TQFP',
    ]);
    assert.equal(
        model.preferredVisiblePackageName(packages, 'STX32 (48-pin VQFN)'),
        '48-PIN VQFN',
    );
});

test('package visibility collapses harmless label drift before preferring the overlay', () => {
    const packages = {
        'STX32 (48-pin VQFN)': {
            pin_count: 48,
            source: 'edc',
            display_name: '48-PIN VQFN',
        },
        '48-PIN VQFN': {
            pin_count: 48,
            source: 'overlay',
            display_name: '48‑PIN  VQFN ',
        },
    };

    assert.deepEqual(model.visiblePackageNames(packages), [
        '48-PIN VQFN',
    ]);
    assert.equal(
        model.preferredVisiblePackageName(packages, 'STX32 (48-pin VQFN)'),
        '48-PIN VQFN',
    );
    assert.equal(
        model.packageIdentity('48-PIN VQFN', packages['48-PIN VQFN']).displayName,
        '48-PIN VQFN',
    );
});

test('package identity drops trailing part-family qualifiers from explicit display names', () => {
    const entry = model.packageIdentity('STX32 (48-pin VQFN)', {
        pin_count: 48,
        source: 'edc',
        display_name: '48-PIN VQFN/TQFP (dsPIC33AKXXXMPS505/dsPIC33AKXXXMPS205)',
    });

    assert.equal(entry.displayName, '48-PIN VQFN/TQFP');
    assert.equal(entry.identityKey, '48|48-PIN VQFN/TQFP');
});

test('peripheral instance extraction stays aligned across the UI', () => {
    assert.deepEqual(model.extractPeripheralInstance('U3TX'), {
        type: 'UART',
        instance: '3',
        id: 'UART3',
    });
    assert.deepEqual(model.extractPeripheralInstance('QEIHOME1'), {
        type: 'QEI',
        instance: '1',
        id: 'QEI1',
    });
    assert.deepEqual(model.extractPeripheralInstance('CLC2OUT'), {
        type: 'CLC',
        instance: '2',
        id: 'CLC2',
    });
    assert.deepEqual(model.extractPeripheralInstance('AD4AN3'), {
        type: 'ADC',
        instance: '4',
        id: 'ADC4',
    });
    assert.equal(model.extractPeripheralInstance('MCLR'), null);
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
    assert.deepEqual(
        model.oscillatorManagedFuseFields('frc_pll', 'EC', 'DSPIC33AK64MC105'),
        [],
    );
    assert.equal(
        model.oscillatorTargetHint(200, 'DSPIC33CK64MP102'),
        'Fcy = 100.000 MHz',
    );
    assert.equal(
        model.oscillatorTargetHint(200, 'DSPIC33AK64MC105'),
        'Fcy = 200.000 MHz (dsPIC33AK: Fcy = Fosc)',
    );
});

test('device profiles capture AK branch and instruction-clock behavior', () => {
    assert.deepEqual(model.resolveDeviceProfile('DSPIC33AK256MPS205'), {
        partNumber: 'DSPIC33AK256MPS205',
        family: 'dspic33',
        architecture: 'dspic33ak',
        branch: 'MPS',
        series: 'dspic33ak-mps',
        instructionClock: 'fosc',
        managesOscillatorFuses: false,
    });
    assert.equal(model.resolveDeviceProfile('DSPIC33CK64MP102').instructionClock, 'fosc_div_2');
});

test('fuse definitions are deduplicated and grouped into higher-signal sections', () => {
    const fuseDefs = [
        {
            cname: 'FICD',
            desc: 'Debugger register',
            default_value: 0,
            fields: [
                { cname: 'JTAGEN', desc: 'JTAG enable', hidden: false, mask: 1, values: [] },
            ],
        },
        {
            cname: 'FWDT',
            desc: 'Watchdog register',
            default_value: 0,
            fields: [
                { cname: 'WDTEN', desc: 'WDT enable', hidden: false, mask: 1, values: [] },
            ],
        },
        {
            cname: 'FICD',
            desc: 'Duplicate debugger register',
            default_value: 0,
            fields: [
                { cname: 'JTAGEN', desc: 'JTAG enable', hidden: false, mask: 1, values: [] },
            ],
        },
    ];

    assert.equal(model.normalizeFuseDefinitions(fuseDefs).length, 2);
    assert.equal(model.visibleFuseFieldCount(fuseDefs), 2);
    assert.deepEqual(
        model.groupedFuseDefinitions(fuseDefs).map(group => ({
            id: group.id,
            registers: group.registers.map(register => register.cname),
        })),
        [
            { id: 'debug', registers: ['FICD'] },
            { id: 'watchdog', registers: ['FWDT'] },
        ],
    );
});

test('analog helpers classify inputs, outputs, and shared analog functions', () => {
    assert.equal(model.isAnalogInput('AN7'), true);
    assert.equal(model.isAnalogInput('AD4AN3'), true);
    assert.equal(model.isAnalogInput('AD1ANN2'), true);
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

test('device interface inventory derives pin-exposed blocks from PPS and pin tags', () => {
    const inventory = model.deriveDeviceInterfaceInventory({
        remappable_inputs: [
            { name: 'U1RX' },
            { name: 'U2RX' },
            { name: 'U3RX' },
            { name: 'SDI1' },
            { name: 'SCK2' },
            { name: 'SENT1' },
            { name: 'SENT2' },
            { name: 'QEIA1' },
            { name: 'QEIHOME1' },
            { name: 'T1CK' },
            { name: 'CLCINA' },
        ],
        remappable_outputs: [
            { name: 'U1TX' },
            { name: 'U2TX' },
            { name: 'U3TX' },
            { name: 'SDO1' },
            { name: 'SCK2' },
            { name: 'PWM1H' },
            { name: 'PWM4L' },
            { name: 'OCM1' },
            { name: 'OCM4' },
            { name: 'CLC4OUT' },
            { name: 'SENT2TX' },
        ],
        pins: [
            { functions: ['ASCL1', 'ASDA1', 'AN7', 'CMP1A', 'OA1IN+', 'DACOUT'] },
            { functions: ['ASCL2', 'ASDA2', 'AN8', 'CMP2B', 'OA2OUT'] },
            { functions: ['AN12', 'CMP3C', 'OA3OUT'] },
        ],
    });

    assert.equal(inventory.uarts, 3);
    assert.equal(inventory.spis, 2);
    assert.equal(inventory.i2c, 2);
    assert.equal(inventory.sent, 2);
    assert.equal(inventory.qei, 1);
    assert.equal(inventory.timers, 1);
    assert.equal(inventory.clc, 1);
    assert.equal(inventory.pwm, 2);
    assert.equal(inventory.sccp, 2);
    assert.equal(inventory.comparators, 3);
    assert.equal(inventory.opAmps, 3);
    assert.equal(inventory.dacs, 1);
    assert.equal(inventory.adcChannels, 3);

    assert.deepEqual(inventory.labels.uarts, ['UART1', 'UART2', 'UART3']);
    assert.deepEqual(inventory.labels.i2c, ['I2C1', 'I2C2']);
    assert.deepEqual(inventory.labels.sent, ['SENT1', 'SENT2']);
    assert.deepEqual(inventory.labels.qei, ['QEI1']);
    assert.deepEqual(inventory.labels.sccp, ['SCCP1', 'SCCP4']);
    assert.deepEqual(inventory.labels.adcChannels, ['AN7', 'AN8', 'AN12']);
});
