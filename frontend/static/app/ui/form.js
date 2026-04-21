/*
 * Form helpers.
 *
 * Hosts PickleUI.select — a custom select trigger that delegates its
 * popover rendering to PickleUI.dropdown so every menu-style overlay in
 * the app shares one primitive.
 */
(function initForm(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});

    function select(trigger, opts) {
        if (!trigger) throw new Error('PickleUI.select: trigger required');
        const options = Array.isArray(opts && opts.options) ? opts.options : [];
        const onSelect = (opts && opts.onSelect) || (() => {});
        const placement = (opts && opts.placement) === 'top' ? 'top-start' : 'bottom-start';

        trigger.classList.add('select-trigger');
        trigger.setAttribute('aria-haspopup', 'listbox');
        trigger.setAttribute('aria-expanded', 'false');
        let current = null;

        function labelFor(value) {
            const m = options.find((o) => o.value === value);
            return m ? m.label : '';
        }

        if (!trigger.textContent && options[0]) trigger.textContent = options[0].label;

        PickleUI.dropdown(trigger, {
            items: () => options.map((o) => ({
                id: o.value,
                label: o.label,
                icon: o.icon,
                active: o.value === current,
            })),
            placement,
            onSelect: (value) => {
                current = value;
                trigger.textContent = labelFor(value) || trigger.textContent;
                onSelect(value);
            },
        });

        return {
            setValue(value) { current = value; trigger.textContent = labelFor(value) || trigger.textContent; },
            getValue() { return current; },
        };
    }

    PickleUI.select = select;
})(typeof window !== 'undefined' ? window : globalThis);
