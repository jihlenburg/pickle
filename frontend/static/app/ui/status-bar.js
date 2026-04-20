/*
 * Status bar helper.
 *
 * Single API: PickleUI.status(text, tone).
 * tone ∈ 'idle' | 'busy' | 'success' | 'warn' | 'error'. Previous tone
 * class is cleared before the new one is applied.
 */
(function initStatus(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});
    const TONES = ['idle', 'busy', 'success', 'warn', 'error'];

    function setStatus(text, tone) {
        const el = global.document.getElementById('status');
        if (!el) return;
        const normalized = TONES.includes(tone) ? tone : 'idle';
        el.textContent = String(text == null ? '' : text);
        for (const t of TONES) {
            el.classList.remove('status-bar-tone-' + t);
        }
        el.classList.add('status-bar-tone-' + normalized);
    }

    PickleUI.status = setStatus;
})(typeof window !== 'undefined' ? window : globalThis);
