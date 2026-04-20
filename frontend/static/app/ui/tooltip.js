/*
 * Tooltip helper.
 *
 * Installs a single reusable tooltip element; attaches pointer/focus
 * listeners at the document level; captures [title] into [data-tip] on
 * first sighting so the native tooltip does not double-render.
 */
(function initTooltip(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});
    const DELAY_MS = 300;

    let element = null;
    let arrow = null;
    let timer = null;
    let installed = false;

    function ensureElement() {
        if (element) return;
        const doc = global.document;
        element = doc.createElement('div');
        element.classList.add('tooltip');
        arrow = doc.createElement('div');
        arrow.classList.add('tooltip-arrow');
        element.appendChild(arrow);
        doc.body.appendChild(element);
    }

    function capture(el) {
        if (!el) return;
        if (el.dataset && el.dataset.tip) {
            if (el.getAttribute && el.getAttribute('title')) {
                el.removeAttribute('title');
            }
            return;
        }
        const title = el.getAttribute && el.getAttribute('title');
        if (title) {
            el.dataset.tip = title;
            el.removeAttribute('title');
        }
    }

    function show(target) {
        if (!target || !target.dataset || !target.dataset.tip) return;
        ensureElement();
        const doc = global.document;
        element.textContent = target.dataset.tip;
        // Re-append arrow because textContent reset cleared children.
        element.appendChild(arrow);
        element.classList.add('is-visible');

        const rect = target.getBoundingClientRect();
        const tipH = element.offsetHeight;
        const tipW = element.offsetWidth;
        let top = rect.top - tipH - 6;
        let above = true;
        if (top < 4) {
            top = rect.bottom + 6;
            above = false;
        }
        let left = rect.left;
        const maxLeft = global.innerWidth - tipW - 4;
        if (left > maxLeft) left = maxLeft;
        if (left < 4) left = 4;

        element.classList.toggle('is-above', above);
        element.classList.toggle('is-below', !above);
        element.style.top = top + 'px';
        element.style.left = left + 'px';
    }

    function hide() {
        if (!element) return;
        element.classList.remove('is-visible');
    }

    function onOver(event) {
        const el = event.target && event.target.closest && event.target.closest('[data-tip], [title]');
        if (!el) return;
        capture(el);
        if (timer) clearTimeout(timer);
        timer = setTimeout(() => show(el), DELAY_MS);
    }

    function onOut(event) {
        const el = event.target && event.target.closest && event.target.closest('[data-tip]');
        if (!el) return;
        if (timer) { clearTimeout(timer); timer = null; }
        hide();
    }

    function install() {
        if (installed) return;
        installed = true;
        const doc = global.document;
        // Initial sweep: capture every [title] currently in the DOM so the
        // native tooltip never appears.
        if (doc.querySelectorAll) {
            for (const el of doc.querySelectorAll('[title]')) capture(el);
        }
        doc.addEventListener('mouseover', onOver);
        doc.addEventListener('mouseout', onOut);
        doc.addEventListener('focusin', onOver);
        doc.addEventListener('focusout', onOut);
    }

    PickleUI.tooltip = { install, capture, show, hide };
})(typeof window !== 'undefined' ? window : globalThis);
