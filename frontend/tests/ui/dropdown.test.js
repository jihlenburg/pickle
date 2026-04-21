const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function load() {
    const ns = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const d = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'dropdown.js'), 'utf8');
    return ns + '\n' + d;
}

function mkDoc() {
    const docListeners = {};
    const body = { _children: [], appendChild(el) { this._children.push(el); el.parentNode = this; }, contains(e) { return this._children.includes(e); } };
    function element(tag) {
        const cl = new Set();
        const l = {};
        const el = {
            tagName: tag.toUpperCase(), children: [], textContent: '', attributes: {}, dataset: {}, style: {}, parentNode: null,
            classList: { add: (c) => cl.add(c), remove: (c) => cl.delete(c), contains: (c) => cl.has(c) },
            appendChild(c) { this.children.push(c); c.parentNode = this; },
            setAttribute(n, v) { this.attributes[n] = v; },
            getAttribute(n) { return this.attributes[n]; },
            addEventListener(t, fn) { (l[t] ||= []).push(fn); },
            click() { for (const fn of (l.click || [])) fn({ target: el, stopPropagation() {}, preventDefault() {} }); },
            getBoundingClientRect() { return { left: 0, top: 0, right: 120, bottom: 28, width: 120, height: 28 }; },
            contains(x) { return x === el || this.children.some((c) => c.contains && c.contains(x)); },
            remove() { if (this.parentNode) { const arr = this.parentNode._children || this.parentNode.children; const i = arr.indexOf(this); if (i !== -1) arr.splice(i, 1); } this.parentNode = null; },
        };
        return el;
    }
    return {
        body,
        createElement: element,
        addEventListener(t, fn) { (docListeners[t] ||= []).push(fn); },
        removeEventListener(t, fn) { const a = docListeners[t] || []; const i = a.indexOf(fn); if (i !== -1) a.splice(i, 1); },
        dispatch(t, ev) { for (const fn of (docListeners[t] || [])) fn(ev); },
    };
}

test('PickleUI.dropdown opens on trigger click, fires onSelect, closes', () => {
    const source = load();
    const doc = mkDoc();
    const trigger = doc.createElement('button');
    const sandbox = { window: {}, document: doc };
    sandbox.window.document = doc;
    sandbox.window.innerHeight = 600;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const received = [];
    sandbox.window.PickleUI.dropdown(trigger, {
        items: [{ id: 'a', label: 'Alpha' }, { id: 'b', label: 'Beta' }],
        onSelect: (id) => received.push(id),
    });

    trigger.click();
    const menu = doc.body._children[doc.body._children.length - 1];
    assert.ok(menu);
    assert.equal(menu.children.length, 2);

    menu.children[1].click();
    assert.deepEqual(received, ['b']);
    assert.equal(doc.body._children.includes(menu), false, 'menu removed after select');
});

test('PickleUI.dropdown closes on Esc', () => {
    const source = load();
    const doc = mkDoc();
    const trigger = doc.createElement('button');
    const sandbox = { window: {}, document: doc };
    sandbox.window.document = doc;
    sandbox.window.innerHeight = 600;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.dropdown(trigger, {
        items: [{ id: 'a', label: 'Alpha' }],
        onSelect: () => {},
    });
    trigger.click();
    assert.ok(doc.body._children.length > 0);
    doc.dispatch('keydown', { key: 'Escape' });
    // Menu is removed; empty or same body with menu no longer present.
    assert.equal(doc.body._children.length === 0 || !doc.body._children[doc.body._children.length - 1].classList.contains('dropdown-menu'), true);
});

test('PickleUI.dropdown renders dividers and danger styling', () => {
    const source = load();
    const doc = mkDoc();
    const trigger = doc.createElement('button');
    const sandbox = { window: {}, document: doc };
    sandbox.window.document = doc;
    sandbox.window.innerHeight = 600;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.dropdown(trigger, {
        items: [
            { id: 'a', label: 'Alpha' },
            { divider: true },
            { id: 'd', label: 'Delete', danger: true },
        ],
        onSelect: () => {},
    });
    trigger.click();
    const menu = doc.body._children[doc.body._children.length - 1];
    assert.equal(menu.children.length, 3);
    assert.equal(menu.children[1].classList.contains('dropdown-divider'), true);
    assert.equal(menu.children[2].classList.contains('is-danger'), true);
});

test('PickleUI.dropdown accepts items as a factory and rebuilds on open', () => {
    const source = load();
    const doc = mkDoc();
    const trigger = doc.createElement('button');
    const sandbox = { window: {}, document: doc };
    sandbox.window.document = doc;
    sandbox.window.innerHeight = 600;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    let includeDelete = false;
    sandbox.window.PickleUI.dropdown(trigger, {
        items: () => {
            const list = [{ id: 'a', label: 'Alpha' }];
            if (includeDelete) list.push({ id: 'd', label: 'Delete', danger: true });
            return list;
        },
        onSelect: () => {},
    });

    trigger.click();
    let menu = doc.body._children[doc.body._children.length - 1];
    assert.equal(menu.children.length, 1, 'factory returns 1 item before state change');
    trigger.click(); // closes

    includeDelete = true;
    trigger.click();
    menu = doc.body._children[doc.body._children.length - 1];
    assert.equal(menu.children.length, 2, 'factory returns 2 items after state change');
});

test('PickleUI.dropdown renders optional item.meta as secondary text', () => {
    const source = load();
    const doc = mkDoc();
    const trigger = doc.createElement('button');
    const sandbox = { window: {}, document: doc };
    sandbox.window.document = doc;
    sandbox.window.innerHeight = 600;
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.dropdown(trigger, {
        items: [{ id: 'a', label: 'Alpha', meta: 'Cached' }],
        onSelect: () => {},
    });
    trigger.click();
    const menu = doc.body._children[doc.body._children.length - 1];
    const metaSpan = menu.children[0].children.find((c) => c.classList.contains('dropdown-item-meta'));
    assert.ok(metaSpan, 'meta span rendered');
    assert.equal(metaSpan.textContent, 'Cached');
});
