/**
 * Guardrails for the unified frontend configuration source.
 *
 * These tests keep both themes structurally aligned and verify that the config
 * helpers still provide the CSS variables the app shell expects at startup.
 */
const test = require('node:test');
const assert = require('node:assert/strict');

const config = require('../static/app/config.js');

test('dark and light themes expose the same CSS variable keys', () => {
    const darkKeys = Object.keys(config.themes.dark).sort();
    const lightKeys = Object.keys(config.themes.light).sort();

    assert.deepEqual(lightKeys, darkKeys);
});

test('theme metadata stays aligned with the configured cycle and labels', () => {
    assert.deepEqual(config.theme.modes, ['dark', 'light', 'system']);
    assert.equal(config.theme.cycle.dark, 'light');
    assert.equal(config.theme.cycle.light, 'system');
    assert.equal(config.theme.cycle.system, 'dark');
    assert.equal(config.theme.labels.dark, 'Dark');
    assert.equal(config.theme.labels.light, 'Light');
    assert.equal(config.theme.labels.system, 'System');
});

test('default toolchain config distinguishes PIC24 and dsPIC33 compilers', () => {
    assert.equal(config.defaults.toolchain.fallbackCompiler, 'xc-dsc-gcc');
    assert.equal(config.defaults.toolchain.familyCompilers.pic24, 'xc16-gcc');
    assert.equal(config.defaults.toolchain.familyCompilers.dspic33, 'xc-dsc-gcc');
    assert.equal(config.defaults.codegen.outputBasename, 'mcu_init');
    assert.equal(config.ui.compiler.familyLabels.pic24, 'PIC24');
    assert.equal(config.ui.compiler.familyLabels.dspic33, 'dsPIC33');
});

test('format helper expands placeholders used by UI copy templates', () => {
    const text = config.format(config.ui.catalog.badgeText, {
        total: 123,
        cached: 45,
        freshness: config.ui.catalog.labels.fresh,
    });

    assert.equal(text, '123 devices | 45 cached | fresh');
});

test('applyDocumentTheme writes the configured CSS variables to the document root', () => {
    const style = {
        values: {},
        setProperty(name, value) {
            this.values[name] = value;
        },
    };
    const documentStub = {
        documentElement: {
            style,
            attributes: {},
            setAttribute(name, value) {
                this.attributes[name] = value;
            },
        },
    };

    config.applyDocumentTheme(documentStub, 'light');

    assert.equal(documentStub.documentElement.attributes['data-theme'], 'light');
    assert.equal(documentStub.documentElement.style.colorScheme, 'light');
    assert.equal(style.values['--font-body'], config.typography.body);
    assert.equal(style.values['--accent'], config.themes.light['--accent']);
    assert.equal(style.values['--status-good'], config.themes.light['--status-good']);
});
