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

test('PickleUI.tooltip.capture prefers existing [data-tip] over [title]', () => {
    const source = loadTooltip();
    const document = fakeDoc();
    const window = { innerWidth: 800, innerHeight: 600, document };
    document.defaultView = window;
    const sandbox = { window, document };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const el = {
        attributes: { title: 'Native' },
        dataset: { tip: 'Custom' },
        getAttribute(n) { return this.attributes[n]; },
        removeAttribute(n) { delete this.attributes[n]; },
        setAttribute(n, v) { this.attributes[n] = v; },
    };

    sandbox.window.PickleUI.tooltip.capture(el);
    assert.equal(el.attributes.title, undefined);
    assert.equal(el.dataset.tip, 'Custom');
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
