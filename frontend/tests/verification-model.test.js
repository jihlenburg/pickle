/**
 * Tests for pure verification helpers.
 *
 * These keep the progress-stage mapping and package matching logic stable
 * without needing a browser DOM.
 */
const test = require('node:test');
const assert = require('node:assert/strict');

globalThis.PickleModel = require('../static/app/model.js');
const verificationModel = require('../static/app/07-verification-model.js');

test('matching packages filters by the loaded device pin count', () => {
    const result = {
        packages: {
            '48-PIN TQFP': { pin_count: 48 },
            '64-PIN TQFP': { pin_count: 64 },
        },
    };

    assert.deepEqual(
        verificationModel.matchingPackages({ pin_count: 48 }, result),
        ['48-PIN TQFP'],
    );
    assert.deepEqual(
        verificationModel.matchingPackages({ pin_count: 0 }, result),
        ['48-PIN TQFP', '64-PIN TQFP'],
    );
});

test('current pin map indexes loaded pins by package position', () => {
    const device = {
        pins: [
            { position: 1, pad_name: 'RA0' },
            { position: 2, pad_name: 'RA1' },
        ],
    };

    assert.deepEqual(verificationModel.currentPinMap(device), {
        1: { position: 1, pad_name: 'RA0' },
        2: { position: 2, pad_name: 'RA1' },
    });
});

test('progress normalization fills defaults from the stage map', () => {
    assert.deepEqual(
        verificationModel.normalizeProgress({
            stage: 'provider.analyze',
            label: 'Analyzing',
            provider: 'OpenAI',
        }),
        {
            stage: 'provider.analyze',
            label: 'Analyzing',
            detail: '',
            progress: 0.76,
            indeterminate: false,
            provider: 'OpenAI',
        },
    );

    assert.equal(
        verificationModel.normalizeProgress('Working...').stage,
        'legacy',
    );
    assert.equal(
        verificationModel.progressStepId('provider.upload'),
        'upload',
    );
});

test('verification package identity uses extracted pin count when matching loaded display-name overrides', () => {
    const device = {
        selected_package: 'STX32 (48-pin VQFN)',
        packages: {
            'STX32 (48-pin VQFN)': {
                pin_count: 48,
                source: 'edc',
                display_name: '48-PIN VQFN/TQFP (dsPIC33AKXXXMPS505/dsPIC33AKXXXMPS205)',
            },
        },
    };
    const extracted = {
        pin_count: 48,
    };

    const loadedIdentity = verificationModel.packageIdentityForVerification(
        device,
        device.selected_package,
    );
    const extractedIdentity = verificationModel.packageIdentityForVerification(
        device,
        '48-PIN VQFN/TQFP (dsPIC33AKXXXMPS505/dsPIC33AKXXXMPS205)',
        extracted,
    );

    assert.equal(loadedIdentity.displayName, '48-PIN VQFN/TQFP');
    assert.equal(extractedIdentity.displayName, '48-PIN VQFN/TQFP');
    assert.equal(loadedIdentity.identityKey, '48|48-PIN VQFN/TQFP');
    assert.equal(extractedIdentity.identityKey, loadedIdentity.identityKey);
});
