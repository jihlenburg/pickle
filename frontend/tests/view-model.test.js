/**
 * Tests for shared pin-presentation helpers.
 *
 * These keep the pin table, package diagram, and peripheral cards aligned on
 * one derived representation of current pin state.
 */
const test = require('node:test');
const assert = require('node:assert/strict');

const viewModel = require('../static/app/02-view-model.js');

test('pinPortLabel prefers port names, then pad names, then function fallback', () => {
    assert.equal(viewModel.pinPortLabel({ port: 'B', port_bit: 7, pad_name: 'RB7' }), 'RB7');
    assert.equal(viewModel.pinPortLabel({ pad_name: 'MCLR', functions: ['MCLR'] }), 'MCLR');
    assert.equal(viewModel.pinPortLabel({ functions: ['AN0'] }), 'AN0');
    assert.equal(viewModel.pinPortLabel(null), '—');
});

test('buildPinPresentation keeps assigned, reserved, and package-label state aligned', () => {
    const pin = { position: 12, port: 'C', port_bit: 3, functions: ['RC3', 'TDO'] };
    const context = {
        signalNames: { 12: 'UART_TX' },
        getAssignmentsAt(position) {
            return position === 12 ? [{ peripheral: 'U1TX', direction: 'out', fixed: false }] : [];
        },
        isIcspPin() {
            return false;
        },
        isJtagPin() {
            return true;
        },
        getJtagFunction() {
            return 'TDO';
        },
    };

    const assigned = viewModel.buildPinPresentation(pin, context);
    assert.equal(assigned.portLabel, 'RC3');
    assert.equal(assigned.signalName, 'UART_TX');
    assert.equal(assigned.assigned, true);
    assert.equal(assigned.jtag, true);
    assert.equal(assigned.blocked, true);
    assert.equal(assigned.packageLabel, 'UART_TX');

    const unassigned = viewModel.buildPinPresentation(pin, {
        ...context,
        signalNames: {},
        getAssignmentsAt() {
            return [];
        },
    });
    assert.equal(unassigned.assigned, false);
    assert.equal(unassigned.packageLabel, 'TDO');
});

test('signalNameForAssignedPeripheral resolves the current pin-level label', () => {
    assert.equal(
        viewModel.signalNameForAssignedPeripheral('U1TX', { U1TX: 8 }, { 8: 'UART_TX' }),
        'UART_TX'
    );
    assert.equal(
        viewModel.signalNameForAssignedPeripheral('U1TX', { U1TX: 8 }, {}),
        ''
    );
    assert.equal(
        viewModel.signalNameForAssignedPeripheral('U1TX', {}, { 8: 'UART_TX' }),
        ''
    );
});
