/**
 * Structural guardrails for split frontend assets.
 *
 * These tests keep the HTML entrypoint, workflow script order, and stylesheet
 * manifest aligned with the file layout expected by the repo docs.
 */
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const frontendRoot = path.join(__dirname, '..');
const htmlPath = path.join(frontendRoot, 'index.html');
const styleManifestPath = path.join(frontendRoot, 'static', 'style.css');

function read(filePath) {
    return fs.readFileSync(filePath, 'utf8');
}

test('index.html loads split workflow modules in the expected order', () => {
    const html = read(htmlPath);
    const scripts = [
        'static/app/05-codegen.js',
        'static/app/05-device-config.js',
        'static/app/05-clc-designer.js',
        'static/app/05-config-files.js',
        'static/app/05-compile-check.js',
        'static/app/06-shell.js',
        'static/app/07-verification.js',
        'static/app/08-bootstrap.js',
    ];

    let lastIndex = -1;
    for (const script of scripts) {
        const currentIndex = html.indexOf(script);
        assert.notEqual(currentIndex, -1, `${script} should be referenced by index.html`);
        assert.ok(currentIndex > lastIndex, `${script} should appear after the previous workflow script`);
        lastIndex = currentIndex;
    }
});

test('style.css imports all split stylesheet modules in order', () => {
    const manifest = read(styleManifestPath);
    const imports = [
        'styles/00-foundation.css',
        'styles/01-pin-code.css',
        'styles/02-package-config.css',
        'styles/03-verify-clc.css',
        'styles/04-shell-layout.css',
        'styles/05-peripheral-responsive.css',
    ];

    let lastIndex = -1;
    for (const stylesheet of imports) {
        const currentIndex = manifest.indexOf(stylesheet);
        assert.notEqual(currentIndex, -1, `${stylesheet} should be imported by style.css`);
        assert.ok(currentIndex > lastIndex, `${stylesheet} should appear after the previous CSS module`);
        lastIndex = currentIndex;

        const absolutePath = path.join(frontendRoot, 'static', stylesheet);
        const stats = fs.statSync(absolutePath);
        assert.ok(stats.isFile(), `${stylesheet} should exist as a file`);
        assert.ok(stats.size > 0, `${stylesheet} should not be empty`);
    }
});
