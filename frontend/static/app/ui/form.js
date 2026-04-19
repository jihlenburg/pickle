/*
 * Form helpers.
 *
 * Hosts PickleUI.select — a custom select popover that renders a
 * .dropdown-menu-shaped list below the trigger. PR #5 refactors this
 * to delegate to PickleUI.dropdown; until then, the popover is inlined
 * here so form.js lands without depending on an unshipped primitive.
 */
(function initForm(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});

    function select(trigger, opts) {
        if (!trigger) {
            throw new Error('PickleUI.select: trigger element required');
        }
        const options = Array.isArray(opts && opts.options) ? opts.options : [];
        const onSelect = (opts && opts.onSelect) || (() => {});
        const placement = (opts && opts.placement) || 'bottom';

        let menu = null;
        let current = null;
        let outsideHandler = null;
        let escHandler = null;

        trigger.classList.add('select-trigger');
        trigger.setAttribute('aria-haspopup', 'listbox');
        trigger.setAttribute('aria-expanded', 'false');
        if (!trigger.textContent) {
            trigger.textContent = options[0] ? options[0].label : '';
        }

        function close() {
            if (!menu) return;
            menu.remove();
            menu = null;
            trigger.setAttribute('aria-expanded', 'false');
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
            menu.setAttribute('role', 'listbox');

            for (const opt of options) {
                const item = doc.createElement('button');
                item.setAttribute('type', 'button');
                item.setAttribute('role', 'option');
                item.classList.add('dropdown-item');
                if (opt.value === current) item.classList.add('is-active');
                item.dataset.value = String(opt.value);
                item.textContent = opt.label;
                item.addEventListener('click', (event) => {
                    event.stopPropagation();
                    setValue(opt.value);
                    onSelect(opt.value);
                    close();
                });
                menu.appendChild(item);
            }

            // Position: below-left of trigger (top placement mirrors the offset).
            const rect = trigger.getBoundingClientRect();
            menu.style.position = 'fixed';
            menu.style.left = rect.left + 'px';
            menu.style.minWidth = Math.max(rect.width, 160) + 'px';
            menu.style.zIndex = 'var(--z-dropdown)';
            if (placement === 'top') {
                menu.style.bottom = (global.innerHeight - rect.top + 4) + 'px';
            } else {
                menu.style.top = (rect.bottom + 4) + 'px';
            }

            doc.body.appendChild(menu);
            trigger.setAttribute('aria-expanded', 'true');

            outsideHandler = (event) => {
                if (!trigger.contains(event.target) && !(menu && menu.contains(event.target))) {
                    close();
                }
            };
            escHandler = (event) => {
                if (event.key === 'Escape') close();
            };
            doc.addEventListener('mousedown', outsideHandler, true);
            doc.addEventListener('keydown', escHandler, true);
        }

        function setValue(value) {
            current = value;
            const match = options.find((o) => o.value === value);
            if (match) trigger.textContent = match.label;
        }

        trigger.addEventListener('click', (event) => {
            event.preventDefault();
            if (menu) close(); else open();
        });

        return { open, close, setValue, getValue: () => current };
    }

    PickleUI.select = select;
})(typeof window !== 'undefined' ? window : globalThis);
