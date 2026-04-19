/*
 * PickleUI namespace.
 *
 * Top-level host for design-system helpers (modal, toast, tooltip,
 * dropdown, tab-strip, form, status-bar). Every ui/*.js file attaches
 * its exports under window.PickleUI and must not overwrite siblings.
 */
(function initPickleUI(global) {
    if (!global.PickleUI || typeof global.PickleUI !== 'object') {
        global.PickleUI = {};
    }
})(typeof window !== 'undefined' ? window : globalThis);
