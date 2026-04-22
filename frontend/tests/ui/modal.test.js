const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function load() {
    const ns = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const m = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'modal.js'), 'utf8');
    return ns + '\n' + m;
}

function makeDialog(id) {
    const cl = new Set(['modal']);
    const listeners = {};
    return {
        id,
        tagName: 'DIALOG',
        attributes: {},
        open: false,
        _focusedAt: null,
        classList: { add: (c) => cl.add(c), remove: (c) => cl.delete(c), contains: (c) => cl.has(c) },
        setAttribute(n, v) { this.attributes[n] = v; },
        getAttribute(n) { return this.attributes[n]; },
        addEventListener(t, fn) { (listeners[t] ||= []).push(fn); },
        removeEventListener(t, fn) { const a = listeners[t] || []; const i = a.indexOf(fn); if (i !== -1) a.splice(i, 1); },
        dispatch(t, ev) { for (const fn of (listeners[t] || [])) fn(ev); },
        showModal() { this.open = true; },
        close() { this.open = false; this.dispatch('close', {}); },
        querySelector() { return null; },
        focus() { this._focusedAt = Date.now(); },
    };
}

test('PickleUI.modal.open calls showModal + restores focus on close', () => {
    const dialog = makeDialog('d1');
    const prior = { focus() { this._restored = true; } };
    const document = {
        getElementById: (id) => (id === 'd1' ? dialog : null),
        activeElement: prior,
    };
    const window = { document };
    const sandbox = { window, document };
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    sandbox.window.PickleUI.modal.open('d1');
    assert.equal(dialog.open, true);
    dialog.close();
    assert.equal(prior._restored, true);
});

test('PickleUI.modal.close warns when dialog id is unknown', () => {
    const document = { getElementById: () => null, activeElement: null };
    const window = { document };
    const sandbox = { window, document };
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);
    sandbox.window.PickleUI.modal.close('does-not-exist');
});

test('PickleUI.modal.confirm resolves true on primary, false on cancel', async () => {
    const sandbox = { window: {}, document: null };
    vm.createContext(sandbox);
    vm.runInContext(load(), sandbox);

    const dialog = makeDialog('pickle-confirm');
    const body = { _children: [], appendChild(el) { this._children.push(el); el.parentNode = this; }, removeChild(el) { const i = this._children.indexOf(el); if (i !== -1) this._children.splice(i, 1); } };
    const doc = {
        body,
        getElementById: (id) => (id === 'pickle-confirm' ? dialog : null),
        activeElement: null,
        createElement: (tag) => {
            const cl = new Set();
            const l = {};
            const el = {
                tagName: tag.toUpperCase(), children: [], textContent: '', attributes: {},
                classList: { add: (c) => cl.add(c), remove: (c) => cl.delete(c), contains: (c) => cl.has(c) },
                appendChild(c) { this.children.push(c); c.parentNode = this; },
                setAttribute(n, v) { this.attributes[n] = v; },
                addEventListener(t, fn) { (l[t] ||= []).push(fn); },
                click() { for (const fn of (l.click || [])) fn({}); },
            };
            if (tag.toLowerCase() === 'dialog') {
                el.showModal = () => { el.open = true; };
                el.close = () => {
                    el.open = false;
                    for (const fn of (l.close || [])) fn({});
                };
            }
            return el;
        },
    };
    sandbox.document = doc;
    sandbox.window.document = doc;

    const p1 = sandbox.window.PickleUI.modal.confirm({
        title: 'Delete?',
        message: 'Really delete?',
        action: 'Delete',
    });
    const createdDialog = body._children[body._children.length - 1];
    const footer = createdDialog.children.find((c) => c.classList.contains('modal-footer'));
    const cancel = footer.children[0];
    const confirm = footer.children[1];
    confirm.click();
    assert.equal(await p1, true);

    const p2 = sandbox.window.PickleUI.modal.confirm({
        title: 'Delete?',
        message: 'Really delete?',
        action: 'Delete',
    });
    const createdDialog2 = body._children[body._children.length - 1];
    const footer2 = createdDialog2.children.find((c) => c.classList.contains('modal-footer'));
    footer2.children[0].click();
    assert.equal(await p2, false);
});
