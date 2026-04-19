const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function loadForm() {
    const namespace = fs.readFileSync(
        path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const form = fs.readFileSync(
        path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'form.js'), 'utf8');
    return namespace + '\n' + form;
}

function makeDom() {
    const listeners = new Map();
    const doc = {
        _els: [],
        body: { _children: [], appendChild(el) { this._children.push(el); } },
        addEventListener(type, fn) {
            (listeners.get(type) || listeners.set(type, []).get(type)).push(fn);
        },
        removeEventListener(type, fn) {
            const arr = listeners.get(type) || [];
            const idx = arr.indexOf(fn);
            if (idx !== -1) arr.splice(idx, 1);
        },
        dispatch(type, event) {
            for (const fn of (listeners.get(type) || [])) fn(event);
        },
        createElement(tag) {
            const el = {
                tagName: tag.toUpperCase(),
                children: [],
                classList: new Set(),
                dataset: {},
                style: {},
                attributes: {},
                textContent: '',
                setAttribute(n, v) { this.attributes[n] = v; },
                removeAttribute(n) { delete this.attributes[n]; },
                getAttribute(n) { return this.attributes[n]; },
                appendChild(c) { this.children.push(c); c.parentNode = this; return c; },
                remove() {
                    if (this.parentNode) {
                        const i = this.parentNode.children.indexOf(this);
                        if (i !== -1) this.parentNode.children.splice(i, 1);
                    }
                },
                addEventListener(t, f) { (this._l ||= {})[t] = ((this._l?.[t]) || []).concat(f); },
                click() { for (const f of (this._l?.click || [])) f({ target: this, stopPropagation() {}, preventDefault() {} }); },
                getBoundingClientRect() { return { left: 0, top: 0, right: 100, bottom: 28, width: 100, height: 28 }; },
                contains(other) { return other === this || this.children.some(c => c.contains && c.contains(other)); },
            };
            el.classList.add = (c) => el.classList instanceof Set ? Set.prototype.add.call(el.classList, c) : null;
            doc._els.push(el);
            return el;
        },
    };
    // Fix classList to behave like DOMTokenList
    const origCreate = doc.createElement;
    doc.createElement = function(tag) {
        const el = origCreate.call(this, tag);
        const set = new Set();
        el.classList = {
            add: (c) => set.add(c),
            remove: (c) => set.delete(c),
            toggle: (c, force) => {
                const has = set.has(c);
                if (force === true || (force === undefined && !has)) set.add(c);
                else set.delete(c);
            },
            contains: (c) => set.has(c),
            _set: set,
        };
        return el;
    };
    return doc;
}

test('PickleUI.select renders options and fires onSelect with value', () => {
    const source = loadForm();
    const document = makeDom();
    const trigger = document.createElement('button');
    const sandbox = { window: {}, document };
    sandbox.window.document = document;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const received = [];
    sandbox.window.PickleUI.select(trigger, {
        options: [{ value: 'a', label: 'Alpha' }, { value: 'b', label: 'Beta' }],
        onSelect: (v) => received.push(v),
    });

    // Clicking the trigger opens the menu (appended to document.body).
    trigger.click();
    const menu = document.body._children[document.body._children.length - 1];
    assert.ok(menu, 'menu appended to body');
    assert.equal(menu.children.length, 2);

    // Clicking the second item fires onSelect('b') and closes the menu.
    menu.children[1].click();
    assert.deepEqual(received, ['b']);
});

test('PickleUI.select labels the trigger with the selected option', () => {
    const source = loadForm();
    const document = makeDom();
    const trigger = document.createElement('button');
    const sandbox = { window: {}, document };
    sandbox.window.document = document;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const handle = sandbox.window.PickleUI.select(trigger, {
        options: [{ value: 'a', label: 'Alpha' }, { value: 'b', label: 'Beta' }],
        onSelect: () => {},
    });

    handle.setValue('b');
    assert.equal(trigger.textContent, 'Beta');
});
