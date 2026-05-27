/*
 * PickleUI namespace.
 *
 * Top-level host for design-system helpers (modal, toast, tooltip,
 * dropdown, tab-strip, form, status-bar). Every ui/*.js file attaches
 * its exports under window.PickleUI and must not overwrite siblings.
 *
 * floatingHost(): returns the element overlays (dropdown menus, tooltips,
 * toasts) must append into so they share the host's stacking context.
 * When a modal <dialog> is open via showModal() it lives in the browser
 * top layer and paints above everything else regardless of z-index; an
 * overlay appended to <body> would render behind the dialog. Routing all
 * floating primitives through this helper keeps menus reachable inside
 * modals (notably the verify-provider select in Settings).
 */
(function initPickleUI(global) {
    if (!global.PickleUI || typeof global.PickleUI !== 'object') {
        global.PickleUI = {};
    }
    if (typeof global.PickleUI.floatingHost !== 'function') {
        global.PickleUI.floatingHost = function floatingHost() {
            const doc = global.document;
            if (!doc) return null;
            if (typeof doc.querySelector === 'function') {
                const dlg = doc.querySelector('dialog[open].modal');
                if (dlg) return dlg;
            }
            return doc.body || null;
        };
    }
})(typeof window !== 'undefined' ? window : globalThis);
