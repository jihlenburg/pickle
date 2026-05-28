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

function mkWindow() {
    return {
        addEventListener() {}, // no-op; tests drive showWindowOnce directly
    };
}

test('hideBootSplash enforces minimum splash display before fade, then removes after transition', () => {
    const { document, splash, removed } = mkDoc();
    const timeouts = [];
    const sandbox = {
        window: mkWindow(),
        document,
        setTimeout: (fn, ms) => { timeouts.push({ fn, ms }); return timeouts.length; },
        clearTimeout: () => {},
    };
    vm.createContext(sandbox);
    vm.runInContext(loadBootstrap(), sandbox);

    // Splash was never marked visible (splashVisibleSince is null) — the
    // function should treat that as "minimum hold already elapsed" and
    // schedule the fade with holdMs=0, then schedule the remove at 200ms
    // after the fade starts.
    sandbox.hideBootSplash();
    assert.equal(splash.classList.contains('is-hidden'), false,
        'is-hidden NOT added synchronously (hold timeout deferred)');
    assert.equal(timeouts.length, 1, 'one outer hold timeout scheduled');
    assert.equal(timeouts[0].ms, 0, 'hold delay is 0 when splashVisibleSince is unset');

    timeouts[0].fn();
    assert.equal(splash.classList.contains('is-hidden'), true,
        'is-hidden added after hold timeout fires');
    assert.equal(removed.value, false, 'element not removed yet (fade timeout pending)');
    assert.equal(timeouts.length, 2, 'inner remove timeout now scheduled');
    assert.equal(timeouts[1].ms, 200,
        'remove scheduled at 200ms (150ms fade + 50ms safety margin)');

    timeouts[1].fn();
    assert.equal(removed.value, true, 'element removed after the fade timeout fires');
});

test('hideBootSplash holds for the remaining time when the splash was shown recently', () => {
    const { document } = mkDoc();
    const timeouts = [];
    let now = 700;
    const sandbox = {
        window: mkWindow(),
        document,
        performance: { now: () => now },
        setTimeout: (fn, ms) => { timeouts.push({ fn, ms }); return timeouts.length; },
        clearTimeout: () => {},
    };
    vm.createContext(sandbox);
    vm.runInContext(loadBootstrap(), sandbox);

    // Drive splashVisibleSince through the real code path: showWindowOnce
    // records performance.now() the first time it's invoked. (__TAURI__ is
    // absent in the sandbox; optional chaining + try/catch make show() a
    // no-op, but splashVisibleSince is still set.)
    sandbox.showWindowOnce();
    // Advance the simulated clock 300ms into the splash's visible life.
    now = 1000;

    sandbox.hideBootSplash();
    assert.equal(timeouts.length, 1, 'one hold timeout scheduled');
    assert.equal(timeouts[0].ms, 500,
        'hold = MIN_DISPLAY (800) − elapsed (300) = 500ms');
});

test('hideBootSplash is a no-op when the splash element is absent', () => {
    const sandbox = {
        window: mkWindow(),
        document: { getElementById: () => null },
        setTimeout: () => 1,
        clearTimeout: () => {},
    };
    vm.createContext(sandbox);
    vm.runInContext(loadBootstrap(), sandbox);

    assert.doesNotThrow(() => sandbox.hideBootSplash());
});
