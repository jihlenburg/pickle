const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function loadBootstrap() {
    return fs.readFileSync(
        path.join(__dirname, '..', 'static', 'app', '08-bootstrap.js'),
        'utf8',
    );
}

function mkDoc() {
    const removed = { value: false };
    const splash = {
        id: 'boot-splash',
        classes: new Set(),
        classList: {
            add(c) { splash.classes.add(c); },
            contains(c) { return splash.classes.has(c); },
        },
        remove() { removed.value = true; },
    };
    return {
        document: {
            getElementById(id) { return id === 'boot-splash' ? splash : null; },
        },
        splash,
        removed,
    };
}

test('hideBootSplash adds is-hidden class and removes the element after the transition', () => {
    const { document, splash, removed } = mkDoc();
    const sandbox = {
        window: {},
        document,
        setTimeout: (fn, ms) => { sandbox._lastTimeout = { fn, ms }; return 1; },
        clearTimeout: () => {},
    };
    vm.createContext(sandbox);
    vm.runInContext(loadBootstrap(), sandbox);

    sandbox.hideBootSplash();
    assert.equal(splash.classList.contains('is-hidden'), true,
        'is-hidden class added before removal');
    assert.equal(removed.value, false, 'element not removed synchronously');
    assert.equal(sandbox._lastTimeout.ms, 200,
        'removal scheduled at 200ms (150ms fade + 50ms safety margin)');

    sandbox._lastTimeout.fn();
    assert.equal(removed.value, true, 'element removed after timeout fires');
});

test('hideBootSplash is a no-op when the splash element is absent', () => {
    const sandbox = {
        window: {},
        document: { getElementById: () => null },
        setTimeout: () => 1,
        clearTimeout: () => {},
    };
    vm.createContext(sandbox);
    vm.runInContext(loadBootstrap(), sandbox);

    assert.doesNotThrow(() => sandbox.hideBootSplash());
});
