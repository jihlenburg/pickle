const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

function load() {
    const ns = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', '00-namespace.js'), 'utf8');
    const ts = fs.readFileSync(path.join(__dirname, '..', '..', 'static', 'app', 'ui', 'tab-strip.js'), 'utf8');
    return ns + '\n' + ts;
}

function makeContainer(ids, activeId) {
    const children = ids.map((id) => {
        const cl = new Set(['tab-strip-item']);
        if (id === activeId) cl.add('is-active');
        const l = {};
        const el = {
            tagName: 'BUTTON',
            dataset: { tabId: id },
            attributes: {},
            classList: { add: (c) => cl.add(c), remove: (c) => cl.delete(c), contains: (c) => cl.has(c), _set: cl },
            setAttribute(n, v) { this.attributes[n] = v; },
            getAttribute(n) { return this.attributes[n]; },
            addEventListener(t, fn) { (l[t] ||= []).push(fn); },
            click() { for (const fn of (l.click || [])) fn({ target: el, preventDefault() {} }); },
        };
        return el;
    });
    return {
        classList: { add: () => {}, contains: () => false },
        children,
        querySelectorAll(sel) {
            if (sel === '.tab-strip-item') return children;
            return [];
        },
    };
}

test('PickleUI.tabStrip sets aria roles and initial active state', () => {
    const source = load();
    const container = makeContainer(['a', 'b', 'c'], 'b');
    const sandbox = { window: {} };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.tabStrip(container, { onChange: () => {} });
    assert.equal(container.children[0].attributes.role, 'tab');
    assert.equal(container.children[1].attributes['aria-selected'], 'true');
    assert.equal(container.children[0].attributes['aria-selected'], 'false');
});

test('PickleUI.tabStrip fires onChange and toggles is-active', () => {
    const source = load();
    const container = makeContainer(['a', 'b', 'c'], 'a');
    const seen = [];
    const sandbox = { window: {} };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    sandbox.window.PickleUI.tabStrip(container, { onChange: (id) => seen.push(id) });
    container.children[2].click();
    assert.deepEqual(seen, ['c']);
    assert.equal(container.children[0].classList.contains('is-active'), false);
    assert.equal(container.children[2].classList.contains('is-active'), true);
});

test('PickleUI.tabStrip.activate can programmatically select', () => {
    const source = load();
    const container = makeContainer(['a', 'b'], 'a');
    const seen = [];
    const sandbox = { window: {} };
    vm.createContext(sandbox);
    vm.runInContext(source, sandbox);

    const handle = sandbox.window.PickleUI.tabStrip(container, { onChange: (id) => seen.push(id) });
    handle.activate('b', { silent: true });
    assert.equal(container.children[1].classList.contains('is-active'), true);
    assert.deepEqual(seen, []);
});
