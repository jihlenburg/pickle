const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const scriptPath = path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js');

test('ui/00-namespace.js creates window.PickleUI', () => {
    const source = fs.readFileSync(scriptPath, 'utf8');
    const sandbox = { window: {} };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);
    assert.equal(typeof sandbox.window.PickleUI, 'object');
});

test('ui/00-namespace.js does not overwrite an existing PickleUI', () => {
    const source = fs.readFileSync(scriptPath, 'utf8');
    const existing = { preserved: true };
    const sandbox = { window: { PickleUI: existing } };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);
    assert.equal(sandbox.window.PickleUI, existing);
    assert.equal(sandbox.window.PickleUI.preserved, true);
});
