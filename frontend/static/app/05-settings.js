/**
 * Settings dialog — section-based, extendable settings UI.
 *
 * Architecture
 * ────────────
 * The dialog uses a sidebar / content-area layout.  Each *section* is a
 * pair of DOM nodes that share a `data-section` attribute:
 *
 *   nav button:  <button class="settings-nav-btn" data-section="id">
 *   content div: <div class="settings-section" data-section="id">
 *
 * Clicking a nav button activates the matching content div (and
 * deactivates the rest).  To add a new section later, just drop a nav
 * button and a content div into the HTML — the wiring code below picks
 * them up automatically via `querySelectorAll`.
 *
 * API Keys
 * ────────
 * Keys are persisted to the OS credential store via Tauri commands:
 *
 *   save_api_key({ provider, key })   → stores in keychain
 *   delete_api_key({ provider })      → removes from keychain
 *   api_key_details()                 → returns per-provider status
 *
 * The status display shows where each key was resolved from (keychain,
 * env var, or .env file) plus a last-four-character hint.
 */

/* global $, invoke, setStatus, checkApiKey */
/* eslint-disable no-unused-vars */

// ── Section switching ──────────────────────────────────────────────────

/**
 * Activate the settings section matching `sectionId`.  Both the nav
 * button and the content panel are toggled.
 */
function switchSettingsSection(sectionId) {
    const nav = $('settings-nav');
    const content = $('settings-content');
    if (!nav || !content) return;

    nav.querySelectorAll('.settings-nav-btn').forEach((btn) => {
        btn.classList.toggle('active', btn.dataset.section === sectionId);
    });
    content.querySelectorAll('.settings-section').forEach((sec) => {
        sec.classList.toggle('active', sec.dataset.section === sectionId);
    });
}

// ── API-key helpers ────────────────────────────────────────────────────

const KEY_PROVIDERS = ['openai', 'anthropic'];
const VERIFY_PROVIDERS = ['auto', 'openai', 'anthropic'];

function normalizedVerificationProvider(provider) {
    const normalized = String(provider || '').trim().toLowerCase();
    return VERIFY_PROVIDERS.includes(normalized) ? normalized : 'auto';
}

/** Render a single provider's status badge + hint into its status div. */
function renderKeyStatus(provider, status) {
    const el = $(`key-status-${provider}`);
    if (!el) return;

    if (!status.configured) {
        el.textContent = 'Not configured';
        return;
    }

    const badge = document.createElement('span');
    badge.className = `key-status-badge ${status.source}`;
    badge.textContent = status.source;

    el.textContent = '';
    el.appendChild(badge);
    el.appendChild(document.createTextNode(` ${status.hint}`));
}

/** Fetch current key status for all providers and update the UI. */
async function refreshKeyStatuses() {
    try {
        const details = await invoke('api_key_details');
        for (const provider of KEY_PROVIDERS) {
            renderKeyStatus(provider, details[provider]);
        }
    } catch (err) {
        console.error('Failed to load key status:', err);
    }
}

function renderVerificationProviderStatus(provider) {
    const el = $('verify-provider-status');
    if (!el) return;

    const normalized = normalizedVerificationProvider(provider);
    if (normalized === 'openai') {
        el.textContent = 'Only the OpenAI key will be used for datasheet verification. If no OpenAI key is configured, verification will fail.';
        return;
    }
    if (normalized === 'anthropic') {
        el.textContent = 'Only the Anthropic key will be used for datasheet verification. If no Anthropic key is configured, verification will fail.';
        return;
    }
    el.textContent = 'Auto mode prefers OpenAI when both keys are configured, and falls back to Anthropic otherwise.';
}

// Handle returned by PickleUI.select; populated by wireSettingsDialog().
let verifyProviderHandle = null;

function refreshVerificationProvider() {
    if (!verifyProviderHandle) return;

    const provider = normalizedVerificationProvider(appSettings?.verification?.provider);
    verifyProviderHandle.setValue(provider);
    renderVerificationProviderStatus(provider);
}

async function saveVerificationProvider(rawValue) {
    const provider = normalizedVerificationProvider(rawValue);
    try {
        await invoke('set_verify_provider', { provider });
        if (!appSettings.verification) {
            appSettings.verification = { provider };
        } else {
            appSettings.verification.provider = provider;
        }
        renderVerificationProviderStatus(provider);
        setStatus(`Verification provider set to ${provider}.`, 'success');
        if (typeof checkApiKey === 'function') checkApiKey();
    } catch (err) {
        setStatus(`Failed to save verification provider: ${err}`, 'error');
    }
}

/** Save a key for a given provider and refresh statuses. */
async function saveProviderKey(provider) {
    const input = $(`key-input-${provider}`);
    if (!input) return;
    const key = input.value.trim();
    if (!key) {
        setStatus('Enter a key before saving.', 'warn');
        return;
    }
    try {
        await invoke('save_api_key', { provider, key });
        input.value = '';
        setStatus(`${provider} key saved to keychain.`, 'success');
        await refreshKeyStatuses();
        // Refresh the verify-button tooltip in the main UI
        if (typeof checkApiKey === 'function') checkApiKey();
    } catch (err) {
        setStatus(`Failed to save key: ${err}`, 'error');
    }
}

/** Remove a key for a given provider and refresh statuses. */
async function clearProviderKey(provider) {
    try {
        await invoke('delete_api_key', { provider });
        setStatus(`${provider} key removed from keychain.`, 'success');
        await refreshKeyStatuses();
        if (typeof checkApiKey === 'function') checkApiKey();
    } catch (err) {
        setStatus(`Failed to clear key: ${err}`, 'error');
    }
}

// ── Dialog show / hide ─────────────────────────────────────────────────

function showSettingsDialog() {
    const dialog = $('settings-dialog');
    if (!dialog || dialog.open) return;
    refreshKeyStatuses();
    refreshVerificationProvider();
    dialog.showModal();
}

function hideSettingsDialog() {
    const dialog = $('settings-dialog');
    if (!dialog?.open) return;
    dialog.close();
}

// ── Wiring ─────────────────────────────────────────────────────────────

/**
 * Bind all Settings-dialog event listeners.  Called once from
 * `initializeShellChrome()` in 06-shell.js.
 */
function wireSettingsDialog() {
    const dialog = $('settings-dialog');
    if (!dialog) return;

    // Close on backdrop click
    dialog.addEventListener('click', (e) => {
        const rect = dialog.getBoundingClientRect();
        if (e.clientX < rect.left || e.clientX > rect.right ||
            e.clientY < rect.top  || e.clientY > rect.bottom) {
            dialog.close();
        }
    });

    // Done button
    const closeBtn = $('settings-close-btn');
    if (closeBtn) closeBtn.addEventListener('click', () => dialog.close());

    // Section nav
    const nav = $('settings-nav');
    if (nav) {
        nav.addEventListener('click', (e) => {
            const btn = e.target.closest('.settings-nav-btn');
            if (btn?.dataset.section) switchSettingsSection(btn.dataset.section);
        });
    }

    const providerSelectEl = $('verify-provider-select');
    if (providerSelectEl && window.PickleUI && typeof window.PickleUI.select === 'function') {
        verifyProviderHandle = window.PickleUI.select(providerSelectEl, {
            options: [
                { value: 'auto', label: 'Auto (prefer OpenAI)' },
                { value: 'openai', label: 'OpenAI only' },
                { value: 'anthropic', label: 'Anthropic only' },
            ],
            onSelect: (value) => { void saveVerificationProvider(value); },
        });
        const initialProvider = normalizedVerificationProvider(
            appSettings?.verification?.provider);
        verifyProviderHandle.setValue(initialProvider);
    }

    // Per-provider key controls
    for (const provider of KEY_PROVIDERS) {
        // Save
        const saveBtn = $(`key-save-${provider}`);
        if (saveBtn) saveBtn.addEventListener('click', () => void saveProviderKey(provider));

        // Clear
        const clearBtn = $(`key-clear-${provider}`);
        if (clearBtn) clearBtn.addEventListener('click', () => void clearProviderKey(provider));

        // Reveal toggle
        const revealBtn = $(`key-reveal-${provider}`);
        const input = $(`key-input-${provider}`);
        if (revealBtn && input) {
            revealBtn.addEventListener('click', () => {
                const hidden = input.type === 'password';
                input.type = hidden ? 'text' : 'password';
            });
        }

        // Save on Enter
        if (input) {
            input.addEventListener('keydown', (e) => {
                if (e.key === 'Enter') void saveProviderKey(provider);
            });
        }
    }

    // Escape handled natively by <dialog>
}
