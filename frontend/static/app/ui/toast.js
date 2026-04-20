/*
 * Toast helper.
 *
 * Stack limit of 5 visible toasts; oldest auto-dismiss toast (info /
 * success / warn) is evicted when a 6th is pushed. error + progress
 * never auto-dismiss and never auto-evict.
 */
(function initToast(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});
    const STACK_LIMIT = 5;
    const DEFAULT_DURATION = 5000;
    const ICONS = {
        info: 'i',
        success: '\u2713',
        warn: '!',
        error: '\u2716',
        progress: '\u21bb',
    };
    const TITLES = {
        info: 'Info',
        success: 'Success',
        warn: 'Warning',
        error: 'Error',
        progress: 'Working',
    };

    let stack = null;
    const handles = [];

    function ensureStack() {
        if (stack) return stack;
        const doc = global.document;
        stack = doc.createElement('div');
        stack.classList.add('toast-stack');
        doc.body.appendChild(stack);
        return stack;
    }

    function evictIfNeeded() {
        if (handles.length <= STACK_LIMIT) return;
        const idx = handles.findIndex((h) => h.autoDismiss);
        if (idx !== -1) handles[idx].dismiss();
    }

    function toast(message, opts) {
        const options = opts || {};
        const tone = ['info', 'success', 'warn', 'error', 'progress'].includes(options.tone)
            ? options.tone
            : 'info';
        const sticky = !!options.sticky || tone === 'error' || tone === 'progress';
        const duration = typeof options.duration === 'number' ? options.duration : DEFAULT_DURATION;

        const doc = global.document;
        const root = ensureStack();

        const el = doc.createElement('div');
        el.classList.add('toast');
        el.classList.add('toast-' + tone);
        el.setAttribute('role', tone === 'error' ? 'alert' : 'status');

        const iconEl = doc.createElement('span');
        iconEl.classList.add('toast-icon');
        iconEl.textContent = ICONS[tone];
        el.appendChild(iconEl);

        const content = doc.createElement('div');
        const titleEl = doc.createElement('div');
        titleEl.classList.add('toast-title');
        titleEl.textContent = options.title || TITLES[tone];
        content.appendChild(titleEl);
        const bodyEl = doc.createElement('div');
        bodyEl.classList.add('toast-body');
        bodyEl.textContent = String(message == null ? '' : message);
        content.appendChild(bodyEl);
        el.appendChild(content);

        const actions = doc.createElement('div');
        actions.classList.add('toast-actions');
        if (options.action && options.action.label) {
            const btn = doc.createElement('button');
            btn.setAttribute('type', 'button');
            btn.classList.add('btn');
            btn.classList.add('btn-sm');
            btn.classList.add('btn-ghost');
            btn.textContent = options.action.label;
            btn.addEventListener('click', () => {
                try { options.action.onClick && options.action.onClick(); } finally { handle.dismiss(); }
            });
            actions.appendChild(btn);
        }
        const dismissBtn = doc.createElement('button');
        dismissBtn.setAttribute('type', 'button');
        dismissBtn.setAttribute('aria-label', 'Dismiss');
        dismissBtn.classList.add('toast-dismiss');
        dismissBtn.textContent = '\u00d7';
        dismissBtn.addEventListener('click', () => handle.dismiss());
        actions.appendChild(dismissBtn);
        el.appendChild(actions);

        let progressInner = null;
        if (tone === 'progress') {
            const bar = doc.createElement('div');
            bar.classList.add('toast-progress-bar');
            progressInner = doc.createElement('span');
            progressInner.classList.add('toast-progress-bar-inner');
            bar.appendChild(progressInner);
            el.appendChild(bar);
        }

        root.appendChild(el);

        let timer = null;
        const autoDismiss = !sticky;
        const handle = {
            autoDismiss,
            update(next) {
                if (!next) return;
                if (typeof next.title === 'string') titleEl.textContent = next.title;
                if (typeof next.body === 'string' || typeof next.message === 'string') {
                    bodyEl.textContent = next.body != null ? next.body : next.message;
                }
                if (typeof next.progress === 'number' && progressInner) {
                    const pct = Math.max(0, Math.min(1, next.progress));
                    progressInner.style.width = (pct * 100) + '%';
                }
            },
            dismiss() {
                if (timer) { clearTimeout(timer); timer = null; }
                el.remove();
                const i = handles.indexOf(handle);
                if (i !== -1) handles.splice(i, 1);
            },
        };

        handles.push(handle);
        if (autoDismiss) {
            timer = setTimeout(handle.dismiss, duration);
        }
        evictIfNeeded();
        return handle;
    }

    PickleUI.toast = toast;
})(typeof window !== 'undefined' ? window : globalThis);
