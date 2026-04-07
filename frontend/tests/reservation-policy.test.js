/**
 * Tests for pure reservation and conflict policy helpers.
 *
 * These cover the hardware-rule layer behind fuse-coupled pin reservation and
 * conflict reporting without depending on DOM or mutable frontend globals.
 */
const test = require('node:test');
const assert = require('node:assert/strict');

const policy = require('../static/app/01-reservation-policy.js');

test('I2C routing helpers parse and rebuild default and alternate aliases', () => {
    assert.equal(policy.isI2cRoutingFunction('SCL1'), true);
    assert.equal(policy.isI2cRoutingFunction('ASDA2'), true);
    assert.equal(policy.isI2cRoutingFunction('U1TX'), false);

    assert.deepEqual(policy.parseI2cRoutingFunction('ASCL2'), {
        alternate: true,
        role: 'SCL',
        channel: 2,
    });
    assert.deepEqual(policy.parseI2cRoutingFunction('SDA1'), {
        alternate: false,
        role: 'SDA',
        channel: 1,
    });
    assert.equal(policy.parseI2cRoutingFunction('GPIO'), null);

    assert.equal(policy.getI2cRoutingFunctionName(1, 'SCL', false), 'SCL1');
    assert.equal(policy.getI2cRoutingFunctionName(2, 'SDA', true), 'ASDA2');
});

test('ICSP and JTAG helpers match the active debug-pair naming rules', () => {
    assert.equal(policy.isIcspFunctionForPair('PGEC2', 2), true);
    assert.equal(policy.isIcspFunctionForPair('PGED3', 2), false);
    assert.equal(policy.isIcspFunctionForPair('MCLR', 1), true);
    assert.equal(policy.isIcspFunctionForPair('PGC1', null), false);
    assert.equal(policy.isIcspFunctionForPair('PGD1', undefined), false);
    assert.equal(policy.isIcspFunctionForPair('MCLR', null), true);

    assert.equal(policy.isPinInIcspPair({ functions: ['RB0', 'PGD1'] }, 1), true);
    assert.equal(policy.isPinInIcspPair({ functions: ['RB0', 'PGED3'] }, 1), false);
    assert.equal(policy.isPinInIcspPair({ functions: ['RB0', 'PGD1'] }, null), false);
    assert.equal(policy.isPinInIcspPair({ functions: ['MCLR', 'RA0'] }, null), true);

    assert.equal(policy.isJtagFunction('TCK'), true);
    assert.equal(policy.isJtagFunction('PGEC1'), false);
});

test('assignment conflict analysis reports duplicate signals and illegal analog sharing', () => {
    const assignments = {
        5: { peripheral: 'U1TX', direction: 'out' },
        6: { peripheral: 'U1TX', direction: 'out' },
        7: [
            { peripheral: 'AN0', direction: 'in' },
            { peripheral: 'CMP1A', direction: 'in' },
            { peripheral: 'U1RX', direction: 'in' },
        ],
        8: [
            { peripheral: 'OA1OUT', direction: 'out' },
            { peripheral: 'DAC1OUT', direction: 'out' },
        ],
    };

    const result = policy.analyzeAssignmentConflicts(assignments, {
        isAnalogFunction(name) {
            return /^(AN|CMP|OA|DAC)/.test(name);
        },
        isAnalogOutput(name) {
            return /^(OA|DAC)/.test(name);
        },
    });

    assert.equal(result.messages.length, 3);
    assert.match(result.messages[0], /U1TX \(out\) assigned to both pin 5 and pin 6/);
    assert.match(result.messages[1], /Pin 7: analog\/digital conflict/);
    assert.match(result.messages[2], /Pin 8: multiple analog outputs/);
    assert.deepEqual([...result.conflictPins].sort((a, b) => a - b), [5, 6, 7, 8]);
});
