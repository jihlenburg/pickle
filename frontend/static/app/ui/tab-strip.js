// frontend/static/app/ui/tab-strip.js
/*
 * Tab strip helper.
 *
 * Given a container with .tab-strip-item children that each carry
 * [data-tab-id], wires click -> aria-selected toggle + onChange callback.
 * Programmatic .activate(id) is available for keyboard shortcuts and for
 * initial state restoration.
 */
(function initTabStrip(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});

    function tabStrip(container, opts) {
        if (!container) throw new Error('PickleUI.tabStrip: container required');
        const onChange = (opts && opts.onChange) || (() => {});
        const items = Array.from(container.querySelectorAll('.tab-strip-item'));

        container.classList.add('tab-strip');
        container.setAttribute && container.setAttribute('role', 'tablist');

        for (const item of items) {
            item.setAttribute('role', 'tab');
            const selected = item.classList.contains('is-active');
            item.setAttribute('aria-selected', selected ? 'true' : 'false');
            item.setAttribute('tabindex', selected ? '0' : '-1');
            item.addEventListener('click', (event) => {
                event.preventDefault();
                if (item.classList.contains('is-disabled') || item.getAttribute('aria-disabled') === 'true') return;
                activate(item.dataset.tabId);
            });
        }

        function activate(id, flags) {
            const silent = !!(flags && flags.silent);
            let changed = false;
            for (const item of items) {
                const active = item.dataset.tabId === id;
                const was = item.classList.contains('is-active');
                if (active !== was) changed = true;
                if (active) item.classList.add('is-active'); else item.classList.remove('is-active');
                item.setAttribute('aria-selected', active ? 'true' : 'false');
                item.setAttribute('tabindex', active ? '0' : '-1');
            }
            if (changed && !silent) onChange(id);
        }

        return { activate };
    }

    PickleUI.tabStrip = tabStrip;
})(typeof window !== 'undefined' ? window : globalThis);
