/**
 * Device-configuration editors outside the main pin views.
 *
 * Owns oscillator controls, dynamic fuse forms, and helper functions used by
 * save/load flows to restore those editor states after a device reload.
 */

// =============================================================================
// Oscillator UI
// =============================================================================

/** Set up oscillator configuration UI — show/hide rows based on clock source selection. */
function setupOscUI() {
    const source = document.getElementById('osc-source');
    const crystalRow = document.getElementById('osc-crystal-row');
    const poscmd = document.getElementById('osc-poscmd');
    const targetRow = document.getElementById('osc-target-row');
    const fcy = document.getElementById('osc-fcy-hint');
    const targetInput = document.getElementById('osc-target');

    source.addEventListener('change', () => {
        const val = source.value;
        const needsCrystal = val === 'pri' || val === 'pri_pll';
        const needsTarget = val === 'frc_pll' || val === 'pri_pll';
        crystalRow.style.display = needsCrystal ? '' : 'none';
        targetRow.style.display = needsTarget ? '' : 'none';
        updateFcyHint();
        syncOscillatorManagedFuseFields();
    });

    targetInput.addEventListener('input', updateFcyHint);
    poscmd.addEventListener('change', syncOscillatorManagedFuseFields);

    function updateFcyHint() {
        const val = parseFloat(targetInput.value);
        if (val > 0) {
            fcy.textContent = `Fcy = ${(val / 2).toFixed(3)} MHz`;
        } else {
            fcy.textContent = '';
        }
    }
}

/** Read current oscillator UI values into a config object for the backend. */
function getOscConfig() {
    const source = document.getElementById('osc-source').value;
    if (!source) return null;
    return {
        source: source,
        targetFoscMhz: parseFloat(document.getElementById('osc-target').value) || 0,
        crystalMhz: parseFloat(document.getElementById('osc-crystal').value) || 0,
        poscmd: document.getElementById('osc-poscmd').value,
    };
}

// =============================================================================
// Configuration Fuse UI
// =============================================================================

/**
 * Startup hook for the fuse panel.
 *
 * The actual form is device-driven and gets rebuilt by buildFuseUI() after a
 * device load, so bootstrap only needs a stable callable here.
 */
function setupFuseUI() {}

/**
 * Build the fuse configuration UI dynamically from device DCR definitions.
 * Each register gets a heading; each non-hidden field gets a select with
 * tooltips. Default values come from the register's reset bitmask.
 */
function buildFuseUI(fuseDefs) {
    const container = document.getElementById('fuse-fields');
    container.innerHTML = '';

    if (!fuseDefs || fuseDefs.length === 0) {
        document.getElementById('fuse-config').style.display = 'none';
        return;
    }

    for (const reg of fuseDefs) {
        const visibleFields = reg.fields.filter(f => !f.hidden);
        if (visibleFields.length === 0) continue;

        const heading = document.createElement('div');
        heading.className = 'fuse-register-heading';
        heading.textContent = reg.cname;
        heading.dataset.tip = reg.desc || reg.cname;
        container.appendChild(heading);

        for (const field of visibleFields) {
            const row = document.createElement('div');
            row.className = 'fuse-row';
            row.dataset.field = field.cname;

            const labelWrap = document.createElement('div');
            labelWrap.className = 'fuse-label-wrap';
            const label = document.createElement('label');
            label.textContent = field.cname;
            label.dataset.tip = field.desc || field.cname;
            labelWrap.appendChild(label);
            if (field.desc) {
                const desc = document.createElement('span');
                desc.className = 'fuse-field-desc';
                desc.textContent = field.desc;
                labelWrap.appendChild(desc);
            }
            const note = document.createElement('span');
            note.className = 'fuse-field-note';
            note.hidden = true;
            labelWrap.appendChild(note);
            if (/^ALTI2C[12]$/.test(field.cname)) {
                const warning = document.createElement('span');
                warning.className = 'fuse-field-warning';
                warning.hidden = true;
                labelWrap.appendChild(warning);
            }
            row.appendChild(labelWrap);

            const select = document.createElement('select');
            select.dataset.register = reg.cname;
            select.dataset.field = field.cname;

            const defaultBits = reg.default_value & field.mask;

            for (const val of field.values) {
                const opt = document.createElement('option');
                opt.value = val.cname;
                opt.textContent = val.cname;
                opt.title = val.desc;
                if (val.value === defaultBits) {
                    opt.selected = true;
                    select.dataset.tip = val.desc;
                }
                select.appendChild(opt);
            }

            select.addEventListener('change', () => {
                const sel = select.options[select.selectedIndex];
                select.dataset.tip = sel?.title || '';
                if (!select.disabled) {
                    select.title = select.dataset.tip;
                }
            });

            row.appendChild(select);
            container.appendChild(row);
        }
    }

    // Re-attach fuse listeners that affect pin reservation/highlighting.
    const icsSelect = getFuseSelect('ICS');
    if (icsSelect) {
        icsSelect.addEventListener('change', () => {
            if (deviceData) {
                if (typeof renderActiveView === 'function') renderActiveView(); else renderDevice();
            }
        });
    }

    for (const field of ['JTAGEN', 'ALTI2C1', 'ALTI2C2']) {
        const select = getFuseSelect(field);
        if (select) {
            select.addEventListener('change', applyFuseReservations);
        }
    }

    updateFuseFieldWarnings();
    syncOscillatorManagedFuseFields();
    document.getElementById('fuse-config').style.display = '';
}

/** Disable fuse rows that are currently owned by the oscillator editor. */
function syncOscillatorManagedFuseFields() {
    const source = document.getElementById('osc-source')?.value || '';
    const poscmd = document.getElementById('osc-poscmd')?.value || '';
    const managedFields = new Set(
        window.PickleModel?.oscillatorManagedFuseFields(source, poscmd) || []
    );
    const managedNote =
        window.PickleConfig?.ui?.fuses?.oscillatorManagedNote
        || 'Managed by the oscillator configuration above.';

    for (const row of document.querySelectorAll('#fuse-fields .fuse-row')) {
        const field = row.dataset.field || '';
        const select = row.querySelector('select');
        const note = row.querySelector('.fuse-field-note');
        const isManaged = managedFields.has(field);

        row.classList.toggle('managed', isManaged);
        if (select) {
            select.disabled = isManaged;
            select.title = isManaged ? managedNote : (select.dataset.tip || '');
        }
        if (note) {
            note.hidden = !isManaged;
            note.textContent = isManaged ? managedNote : '';
        }
    }
}

/** Collect dynamic fuse selections as { selections: { REG: { FIELD: VALUE } } }. */
function getFuseConfig() {
    const selections = {};
    for (const sel of document.querySelectorAll('#fuse-fields select')) {
        const reg = sel.dataset.register;
        const field = sel.dataset.field;
        if (!reg || !field) continue;
        if (!selections[reg]) selections[reg] = {};
        selections[reg][field] = sel.value;
    }
    return { selections };
}

/** Restore oscillator controls from a saved configuration after the device UI is ready. */
function applyOscillatorConfig(oscillator) {
    if (!oscillator) return;
    document.getElementById('osc-source').value = oscillator.source || '';
    document.getElementById('osc-target').value = oscillator.target_fosc_mhz || 200;
    document.getElementById('osc-crystal').value = oscillator.crystal_mhz || 8;
    document.getElementById('osc-poscmd').value = oscillator.poscmd || 'EC';
    document.getElementById('osc-source').dispatchEvent(new Event('change'));
}

/** Re-apply saved fuse selections once buildFuseUI() has created the per-device selects. */
function applyFuseSelections(selections) {
    if (!selections) return;

    for (const [reg, fields] of Object.entries(selections)) {
        for (const [field, value] of Object.entries(fields)) {
            const sel = document.querySelector(
                `#fuse-fields select[data-register="${reg}"][data-field="${field}"]`
            );
            if (!sel) continue;
            sel.value = value;
            sel.dispatchEvent(new Event('change'));
        }
    }
}
