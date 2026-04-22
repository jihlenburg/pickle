// frontend/static/app/ui/modal.js
/*
 * Modal helper.
 *
 * open(id): showModal() on the <dialog> and remember focused element.
 * close(id): close() on the <dialog>; focus restoration happens on the
 *   dialog's 'close' event, so it also fires for Esc and backdrop-click.
 * confirm(opts): build a small transient dialog, return Promise<boolean>.
 */
(function initModal(global) {
    const PickleUI = global.PickleUI || (global.PickleUI = {});
    const focusStack = new Map();

    function open(id, opts) {
        const doc = global.document;
        if (!doc) return;
        const dialog = doc.getElementById(id);
        if (!dialog) { console.warn('PickleUI.modal.open: no dialog', id); return; }
        const prior = doc.activeElement || null;
        focusStack.set(id, prior);
        dialog.showModal && dialog.showModal();
        const onClose = () => {
            dialog.removeEventListener('close', onClose);
            const p = focusStack.get(id);
            focusStack.delete(id);
            if (p && typeof p.focus === 'function') p.focus();
            if (opts && typeof opts.onClose === 'function') opts.onClose();
        };
        dialog.addEventListener('close', onClose);
    }

    function close(id) {
        const doc = global.document;
        if (!doc) return;
        const dialog = doc.getElementById(id);
        if (!dialog) { console.warn('PickleUI.modal.close: no dialog', id); return; }
        if (typeof dialog.close === 'function') dialog.close();
    }

    function confirm(opts) {
        const options = opts || {};
        return new Promise((resolve) => {
            const doc = global.document;
            const dialog = doc.createElement('dialog');
            dialog.classList.add('modal');
            dialog.classList.add('modal-sm');

            const header = doc.createElement('div');
            header.classList.add('modal-header');
            const title = doc.createElement('h2');
            title.classList.add('modal-title');
            title.textContent = options.title || 'Confirm';
            header.appendChild(title);
            dialog.appendChild(header);

            const body = doc.createElement('div');
            body.classList.add('modal-body');
            body.textContent = options.message || '';
            dialog.appendChild(body);

            const footer = doc.createElement('div');
            footer.classList.add('modal-footer');
            const cancel = doc.createElement('button');
            cancel.setAttribute('type', 'button');
            cancel.classList.add('btn');
            cancel.classList.add('btn-secondary');
            cancel.textContent = options.cancel || 'Cancel';
            const confirmBtn = doc.createElement('button');
            confirmBtn.setAttribute('type', 'button');
            confirmBtn.classList.add('btn');
            confirmBtn.classList.add('btn-primary');
            if (options.tone === 'danger') confirmBtn.classList.add('btn-danger');
            confirmBtn.textContent = options.action || 'Confirm';
            footer.appendChild(cancel);
            footer.appendChild(confirmBtn);
            dialog.appendChild(footer);

            let settled = false;
            function settle(value) {
                if (settled) return;
                settled = true;
                resolve(value);
                try { dialog.close(); } catch (_) { /* already closed */ }
            }

            cancel.addEventListener('click', () => settle(false));
            confirmBtn.addEventListener('click', () => settle(true));
            dialog.addEventListener('close', () => {
                settle(false);
                if (dialog.parentNode) dialog.parentNode.removeChild(dialog);
            });

            doc.body.appendChild(dialog);
            if (typeof dialog.showModal === 'function') dialog.showModal();
            confirmBtn.focus && confirmBtn.focus();
        });
    }

    PickleUI.modal = { open, close, confirm };
})(typeof window !== 'undefined' ? window : globalThis);
