const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function loadTooltip() {
    const ns = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const tt = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'tooltip.js'), 'utf8');
    return ns + '\n' + tt;
}

function fakeDoc() {
    const listeners = {};
    const body = { _children: [], appendChild(el) { this._children.push(el); } };
    return {
        body,
        addEventListener(t, fn) { (listeners[t] ||= []).push(fn); },
        removeEventListener(t, fn) { const a = listeners[t] || []; const i = a.indexOf(fn); if (i !== -1) a.splice(i, 1); },
        dispatch(t, ev) { for (const fn of (listeners[t] || [])) fn(ev); },
        createElement(tag) {
            const cl = new Set();
            return {
                tagName: tag.toUpperCase(), style: {}, attributes: {}, children: [], textContent: '',
                classList: { add: (c) => cl.add(c), remove: (c) => cl.delete(c), contains: (c) => cl.has(c) },
                setAttribute(n, v) { this.attributes[n] = v; },
                appendChild(c) { this.children.push(c); },
                getBoundingClientRect() { return { left: 0, top: 0, right: 0, bottom: 0, width: 0, height: 0 }; },
                get offsetHeight() { return 20; },
                get offsetWidth() { return 100; },
            };
        },
    };
}

test('PickleUI.tooltip.install captures title= and strips it', () => {
    const source = loadTooltip();
    const document = fakeDoc();
    const window = { innerWidth: 800, innerHeight: 600, document };
    document.defaultView = window;
    const sandbox = { window, document };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const el = {
        attributes: { title: 'Hello' },
        dataset: {},
        getAttribute(n) { return this.attributes[n]; },
        removeAttribute(n) { delete this.attributes[n]; },
        setAttribute(n, v) { this.attributes[n] = v; },
    };

    sandbox.window.PickleUI.tooltip.capture(el);
    assert.equal(el.attributes.title, undefined);
    assert.equal(el.dataset.tip, 'Hello');
});

test('PickleUI.tooltip.capture refreshes data-tip from live title= updates', () => {
    // Regression: callers like verify-btn (07-verification.js), index-badge
    // (06-shell.js), CLC tab (05-clc-designer.js), and pkg-select (00-core.js)
    // update element.title= dynamically after install()'s initial sweep.
    // capture() must reflect those updates into data-tip on the next hover
    // instead of silently keeping the first-sighting copy.
    const source = loadTooltip();
    const document = fakeDoc();
    const window = { innerWidth: 800, innerHeight: 600, document };
    document.defaultView = window;
    const sandbox = { window, document };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const el = {
        attributes: { title: 'Cross-check pinout against the datasheet' },
        dataset: {},
        getAttribute(n) { return this.attributes[n]; },
        removeAttribute(n) { delete this.attributes[n]; },
        setAttribute(n, v) { this.attributes[n] = v; },
    };
    sandbox.window.PickleUI.tooltip.capture(el);
    assert.equal(el.dataset.tip, 'Cross-check pinout against the datasheet');
    assert.equal(el.attributes.title, undefined, 'native title stripped');

    // Simulate the post-install title= update path (e.g. checkApiKey()).
    el.attributes.title = 'API key configured (OpenAI)';
    sandbox.window.PickleUI.tooltip.capture(el);
    assert.equal(el.dataset.tip, 'API key configured (OpenAI)',
        'data-tip reflects the live title= update');
    assert.equal(el.attributes.title, undefined, 'native title stripped again');
});

test('PickleUI.tooltip.capture leaves data-tip alone when no title= is present', () => {
    // After the initial sweep, hovering an element that never had title=
    // (consumers like 05-clc-designer.js write tab.title only conditionally)
    // shouldn't clear a previously-set data-tip.
    const source = loadTooltip();
    const document = fakeDoc();
    const window = { innerWidth: 800, innerHeight: 600, document };
    document.defaultView = window;
    const sandbox = { window, document };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const el = {
        attributes: {},
        dataset: { tip: 'Previously captured' },
        getAttribute(n) { return this.attributes[n]; },
        removeAttribute(n) { delete this.attributes[n]; },
        setAttribute(n, v) { this.attributes[n] = v; },
    };
    sandbox.window.PickleUI.tooltip.capture(el);
    assert.equal(el.dataset.tip, 'Previously captured');
});

test('PickleUI.tooltip exposes show/hide helpers', () => {
    const source = loadTooltip();
    const document = fakeDoc();
    const window = { innerWidth: 800, innerHeight: 600, document };
    document.defaultView = window;
    const sandbox = { window, document };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    assert.equal(typeof sandbox.window.PickleUI.tooltip.show, 'function');
    assert.equal(typeof sandbox.window.PickleUI.tooltip.hide, 'function');
});
