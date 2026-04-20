const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function loadStatus() {
    const ns = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const sb = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'status-bar.js'), 'utf8');
    return ns + '\n' + sb;
}

function makeStatusEl() {
    const cl = new Set();
    return {
        textContent: '',
        attributes: {},
        classList: {
            add: (c) => cl.add(c),
            remove: (c) => cl.delete(c),
            contains: (c) => cl.has(c),
            _set: cl,
        },
        setAttribute(n, v) { this.attributes[n] = v; },
        getAttribute(n) { return this.attributes[n]; },
    };
}

test('PickleUI.status sets text and tone class', () => {
    const source = loadStatus();
    const statusEl = makeStatusEl();
    const document = { getElementById: (id) => (id === 'status' ? statusEl : null) };
    const sandbox = { window: {}, document };
    sandbox.window.document = document;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.status('Loading...', 'busy');
    assert.equal(statusEl.textContent, 'Loading...');
    assert.equal(statusEl.classList.contains('status-bar-tone-busy'), true);

    sandbox.window.PickleUI.status('Done', 'success');
    assert.equal(statusEl.textContent, 'Done');
    assert.equal(statusEl.classList.contains('status-bar-tone-busy'), false);
    assert.equal(statusEl.classList.contains('status-bar-tone-success'), true);
});

test('PickleUI.status defaults to idle tone', () => {
    const source = loadStatus();
    const statusEl = makeStatusEl();
    const document = { getElementById: (id) => (id === 'status' ? statusEl : null) };
    const sandbox = { window: {}, document };
    sandbox.window.document = document;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.status('Ready');
    assert.equal(statusEl.classList.contains('status-bar-tone-idle'), true);
});
