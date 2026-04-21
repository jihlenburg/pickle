/*
 * Dropdown helper.
 *
 * Renders a floating .dropdown-menu anchored to a trigger element.
 * Items accept { id, label, icon?, meta?, danger?, active?, divider? }.
 * `items` may be an Array or a zero-arg function; when a function,
 * it's re-invoked on every open(), so dynamic state (current
 * selection, visibility) reflects at render time.
 * Menu closes on selection, Esc, or click outside.
 */
(function initDropdown(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});
    const PLACEMENTS = ['bottom-start', 'bottom-end', 'top-start', 'top-end'];

    function dropdown(trigger, opts) {
        if (!trigger) throw new Error('PickleUI.dropdown: trigger required');
        const itemsSource = opts && opts.items;
        const onSelect = (opts && opts.onSelect) || (() => {});
        const placement = PLACEMENTS.includes(opts && opts.placement) ? opts.placement : 'bottom-start';

        function resolveItems() {
            const resolved = typeof itemsSource === 'function' ? itemsSource() : itemsSource;
            return Array.isArray(resolved) ? resolved : [];
        }

        let menu = null;
        let outsideHandler = null;
        let escHandler = null;

        function close() {
            if (!menu) return;
            menu.remove();
            menu = null;
            if (trigger.setAttribute) trigger.setAttribute('aria-expanded', 'false');
            if (outsideHandler) {
                global.document.removeEventListener('mousedown', outsideHandler, true);
                outsideHandler = null;
            }
            if (escHandler) {
                global.document.removeEventListener('keydown', escHandler, true);
                escHandler = null;
            }
        }

        function open() {
            if (menu) return;
            const doc = global.document;
            menu = doc.createElement('div');
            menu.classList.add('dropdown-menu');
            menu.setAttribute('role', 'menu');

            for (const it of resolveItems()) {
                if (it && it.divider) {
                    const div = doc.createElement('div');
                    div.classList.add('dropdown-divider');
                    menu.appendChild(div);
                    continue;
                }
                const item = doc.createElement('button');
                item.setAttribute('type', 'button');
                item.setAttribute('role', 'menuitem');
                item.classList.add('dropdown-item');
                if (it.danger) item.classList.add('is-danger');
                if (it.active) item.classList.add('is-active');
                if (it.icon) {
                    const ic = doc.createElement('span');
                    ic.classList.add('dropdown-item-icon');
                    ic.textContent = it.icon;
                    item.appendChild(ic);
                }
                const label = doc.createElement('span');
                label.textContent = it.label;
                item.appendChild(label);
                if (it.meta) {
                    const metaSpan = doc.createElement('span');
                    metaSpan.classList.add('dropdown-item-meta');
                    metaSpan.textContent = it.meta;
                    item.appendChild(metaSpan);
                }
                item.addEventListener('click', (event) => {
                    event.stopPropagation();
                    close();
                    onSelect(it.id);
                });
                menu.appendChild(item);
            }

            // Position the menu.
            const rect = trigger.getBoundingClientRect();
            menu.style.position = 'fixed';
            menu.style.minWidth = Math.max(rect.width, 160) + 'px';
            const wantTop = placement.startsWith('top');
            const alignEnd = placement.endsWith('end');
            if (wantTop) {
                menu.style.bottom = (global.innerHeight - rect.top + 4) + 'px';
            } else {
                menu.style.top = (rect.bottom + 4) + 'px';
            }
            if (alignEnd) {
                menu.style.right = (global.innerWidth - rect.right) + 'px';
            } else {
                menu.style.left = rect.left + 'px';
            }

            doc.body.appendChild(menu);
            if (trigger.setAttribute) trigger.setAttribute('aria-expanded', 'true');

            outsideHandler = (event) => {
                if (!trigger.contains(event.target) && !menu.contains(event.target)) close();
            };
            escHandler = (event) => { if (event.key === 'Escape') close(); };
            doc.addEventListener('mousedown', outsideHandler, true);
            doc.addEventListener('keydown', escHandler, true);
        }

        if (trigger.addEventListener) {
            trigger.setAttribute && trigger.setAttribute('aria-haspopup', 'menu');
            trigger.setAttribute && trigger.setAttribute('aria-expanded', 'false');
            trigger.addEventListener('click', (event) => {
                event.preventDefault();
                if (menu) close(); else open();
            });
        }

        return { open, close };
    }

    PickleUI.dropdown = dropdown;
})(typeof window !== 'undefined' ? window : globalThis);
