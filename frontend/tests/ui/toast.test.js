const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function load() {
    const ns = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const t = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'toast.js'), 'utf8');
    return ns + '\n' + t;
}

function mkDoc() {
    const body = {
        _children: [],
        appendChild(el) { this._children.push(el); el.parentNode = this; },
    };
    return {
        body,
        _timers: [],
        createElement(tag) {
            const cl = new Set();
            const el = {
                tagName: tag.toUpperCase(), children: [], textContent: '', attributes: {}, style: {},
                classList: { add: (c) => cl.add(c), remove: (c) => cl.delete(c), contains: (c) => cl.has(c) },
                appendChild(c) { this.children.push(c); c.parentNode = this; },
                setAttribute(n, v) { this.attributes[n] = v; },
                addEventListener() {},
                remove() {
                    if (this.parentNode) {
                        const i = this.parentNode._children ? this.parentNode._children.indexOf(this) : this.parentNode.children.indexOf(this);
                        const arr = this.parentNode._children || this.parentNode.children;
                        if (i !== -1) arr.splice(i, 1);
                    }
                },
            };
            return el;
        },
    };
}

function findDescendantByClass(el, cls) {
    if (!el || !el.children) return null;
    for (const c of el.children) {
        if (c.classList && c.classList.contains(cls)) return c;
        const deeper = findDescendantByClass(c, cls);
        if (deeper) return deeper;
    }
    return null;
}

test('PickleUI.toast appends to stack and returns a handle', () => {
    const doc = mkDoc();
    const sandbox = { window: {}, document: doc, setTimeout: (fn, ms) => { doc._timers.push({ fn, ms }); return doc._timers.length; }, clearTimeout: () => {} };
    sandbox.window.document = doc;
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    const handle = sandbox.window.PickleUI.toast('Hello');
    const stack = doc.body._children[0];
    assert.equal(stack.children.length, 1);
    assert.equal(typeof handle.dismiss, 'function');
    assert.equal(typeof handle.update, 'function');
});

test('PickleUI.toast error tone is sticky', () => {
    const doc = mkDoc();
    const sandbox = { window: {}, document: doc, setTimeout: (fn, ms) => { doc._timers.push({ fn, ms }); return doc._timers.length; }, clearTimeout: () => {} };
    sandbox.window.document = doc;
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    sandbox.window.PickleUI.toast('Broke', { tone: 'error' });
    assert.equal(doc._timers.length, 0, 'error toast must not schedule auto-dismiss');
});

test('PickleUI.toast info tone auto-dismisses after 5 s', () => {
    const doc = mkDoc();
    const sandbox = { window: {}, document: doc, setTimeout: (fn, ms) => { doc._timers.push({ fn, ms }); return doc._timers.length; }, clearTimeout: () => {} };
    sandbox.window.document = doc;
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    sandbox.window.PickleUI.toast('Hello', { tone: 'info' });
    assert.equal(doc._timers.length, 1);
    assert.equal(doc._timers[0].ms, 5000);
});

test('PickleUI.toast stack caps visible toasts at 5 and evicts oldest auto-dismiss first', () => {
    const doc = mkDoc();
    const sandbox = { window: {}, document: doc, setTimeout: () => 1, clearTimeout: () => {} };
    sandbox.window.document = doc;
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    for (let i = 0; i < 6; i += 1) {
        sandbox.window.PickleUI.toast('t' + i, { tone: 'info' });
    }
    const stack = doc.body._children[0];
    assert.equal(stack.children.length, 5);
});

test('PickleUI.toast.update mutates title/body/progress', () => {
    const doc = mkDoc();
    const sandbox = { window: {}, document: doc, setTimeout: () => 1, clearTimeout: () => {} };
    sandbox.window.document = doc;
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    const h = sandbox.window.PickleUI.toast('Working', { tone: 'progress', title: 'Verify' });
    h.update({ title: 'Verify done', body: 'All good', progress: 1 });
    // find the toast node in the stack
    const stack = doc.body._children[0];
    const toastEl = stack.children[0];
    // title + body live inside a content wrapper; search recursively
    const titleEl = findDescendantByClass(toastEl, 'toast-title');
    const bodyEl = findDescendantByClass(toastEl, 'toast-body');
    assert.equal(titleEl.textContent, 'Verify done');
    assert.equal(bodyEl.textContent, 'All good');
});

test('PickleUI.toast stack hard-caps sticky toasts at STACK_HARD_CAP', () => {
    const doc = mkDoc();
    const sandbox = { window: {}, document: doc, setTimeout: () => 1, clearTimeout: () => {} };
    sandbox.window.document = doc;
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    for (let i = 0; i < 15; i += 1) {
        sandbox.window.PickleUI.toast('t' + i, { tone: 'error' });
    }
    const stack = doc.body._children[0];
    assert.ok(stack.children.length <= 10,
        `expected at most 10 sticky toasts, got ${stack.children.length}`);
});
