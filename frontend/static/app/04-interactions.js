/**
 * Interactive assignment handlers for the pin and peripheral views.
 *
 * This file handles direct user edits, then triggers the minimal redraws
 * needed to keep the table, peripheral view, conflicts, and diagram in sync.
 */
// =============================================================================
// Assignment Change Handlers — Clickable Function Tags + PPS Remapping
// =============================================================================

/**
 * Handle clicks on function tags in the pin-view Functions column.
 * Analog functions toggle (multi-select allowed); digital/GPIO are exclusive;
 * PPS pseudo-tag focuses the Remapping dropdown.
 */
function onFuncTagClick(e) {
    const tag = e.currentTarget;
    const pinPos = parseInt(tag.dataset.pinPos, 10);
    const fnName = tag.dataset.function;
    const pin = deviceData.pins.find(p => p.position === pinPos);
    if (!pin) return;

    // Guard: reserved tags are not clickable
    if (tag.classList.contains('reserved')) return;

    // PPS pseudo-tag: focus the Remapping dropdown instead of toggling
    if (/^RP\d+$/.test(fnName)) {
        const row = document.getElementById(`pin-row-${pinPos}`);
        const select = row?.querySelector('.remap-select');
        if (select) { select.focus(); select.showPicker?.(); }
        return;
    }

    // Power/supply functions are never assignable
    if (/^V[DS]|^AV[DS]/.test(fnName)) return;

    pushUndo();

    const isAnalog = isAnalogFunction(fnName);
    const alreadyAssigned = hasAssignmentFor(pinPos, fnName);

    if (alreadyAssigned) {
        // Toggle off — remove this specific assignment
        removeAssignment(pinPos, fnName);
        if (!assignments[pinPos]) delete signalNames[pinPos];
    } else if (isAnalog) {
        // Analog toggle on — clear any digital/PPS first, then add
        const existing = getAssignmentsAt(pinPos);
        const digitalAssign = existing.find(a => !isAnalogFunction(a.peripheral));
        if (digitalAssign) {
            removeAssignment(pinPos, digitalAssign.peripheral);
            // Also reset the PPS Remapping dropdown
            const row = document.getElementById(`pin-row-${pinPos}`);
            const select = row?.querySelector('.remap-select');
            if (select) select.value = '';
        }

        const periphs = getAvailablePeripherals(pin);
        const pInfo = periphs.find(p => p.name === fnName);
        addAnalogAssignment(pinPos, {
            peripheral: fnName,
            direction: pInfo ? pInfo.direction : 'in',
            ppsval: null,
            rp_number: pin.rp_number,
            fixed: true,
        });
    } else {
        // Digital / fixed non-analog — exclusive, replaces everything
        const periphs = getAvailablePeripherals(pin);
        const pInfo = periphs.find(p => p.name === fnName);
        setAssignment(pinPos, {
            peripheral: fnName,
            direction: pInfo ? pInfo.direction : 'io',
            ppsval: pInfo ? pInfo.ppsval : null,
            rp_number: pin.rp_number,
            fixed: pInfo ? pInfo.fixed : true,
        });
        // Clear PPS dropdown since a fixed function was chosen
        const row = document.getElementById(`pin-row-${pinPos}`);
        const select = row?.querySelector('.remap-select');
        if (select) select.value = '';
    }

    const row = document.getElementById(`pin-row-${pinPos}`);
    if (row) {
        row.classList.toggle('assigned', !!assignments[pinPos]);
    }

    updateFuncTagStates(pinPos);
    updateSummary();
    checkConflicts();
    renderPackageDiagram();
}

/**
 * Update the .selected class on all function tags in a pin row
 * without triggering a full re-render.
 * @param {number} pinPos - Pin position to update
 */
function updateFuncTagStates(pinPos) {
    const row = document.getElementById(`pin-row-${pinPos}`);
    if (!row) return;

    const tags = row.querySelectorAll('.func-tag.clickable');
    const currentAssigns = getAssignmentsAt(pinPos);
    const hasNonFixed = currentAssigns.some(a => !a.fixed);

    for (const tag of tags) {
        const fn = tag.dataset.function;
        if (!fn) continue;

        if (/^RP\d+$/.test(fn)) {
            // PPS pseudo-tag highlights when a PPS (non-fixed) peripheral is assigned
            tag.classList.toggle('selected', hasNonFixed);
        } else {
            tag.classList.toggle('selected', hasAssignmentFor(pinPos, fn));
        }
    }
}

/**
 * Handle PPS Remapping dropdown changes. Selecting a PPS peripheral replaces
 * all existing assignments (digital/analog); clearing preserves analog.
 */
function onRemapChange(e) {
    pushUndo();

    const select = e.target;
    const pinPos = parseInt(select.dataset.pinPos, 10);
    const rpNum = select.dataset.rpNum ? parseInt(select.dataset.rpNum) : null;
    const row = document.getElementById(`pin-row-${pinPos}`);

    if (select.value) {
        const { name, direction, ppsval } = JSON.parse(select.value);
        // PPS assignment replaces everything (analog + digital)
        setAssignment(pinPos, {
            peripheral: name, direction, ppsval,
            rp_number: rpNum, fixed: false,
        });
        if (row) row.classList.add('assigned');
    } else {
        // Clearing the dropdown: remove only the PPS (non-fixed) assignment,
        // preserve any analog tag selections
        const analogAssigns = getAssignmentsAt(pinPos).filter(a => isAnalogFunction(a.peripheral) && a.fixed);
        delete assignments[pinPos];
        if (analogAssigns.length > 0) {
            assignments[pinPos] = analogAssigns.length === 1 ? analogAssigns[0] : analogAssigns;
        }
        if (row) row.classList.toggle('assigned', !!assignments[pinPos]);
    }

    updateFuncTagStates(pinPos);
    updateSummary();
    checkConflicts();
    renderPackageDiagram();
}

/** Handle pin dropdown changes in the peripheral view. */
function onPeriphAssignChange(e) {
    pushUndo();

    const select = e.target;
    const signalName = select.dataset.signal;
    const signalDir = select.dataset.direction;
    const ppsval = select.dataset.ppsval ? parseInt(select.dataset.ppsval) : null;

    // Remove old assignment for this signal (if any)
    forEachAssignedPin((pinPos, list) => {
        if (list.some(a => a.peripheral === signalName)) {
            removeAssignment(pinPos, signalName);
            if (!assignments[pinPos]) delete signalNames[pinPos];
        }
    });

    // Set new assignment if a pin was selected
    const pinPos = select.value ? parseInt(select.value, 10) : null;
    if (pinPos !== null) {
        const pin = deviceData.pins.find(p => p.position === pinPos);
        if (pin) {
            assignments[pinPos] = {
                peripheral: signalName,
                direction: signalDir,
                ppsval: ppsval,
                rp_number: pin.rp_number,
                fixed: false,
            };
        }

        // Transfer signal name from the input field if present
        const input = document.querySelector(`.periph-signal-input[data-signal="${signalName}"]`);
        if (input && input.value.trim()) {
            signalNames[pinPos] = input.value.trim();
        }
    }

    updateSummary();
    checkConflicts();
    renderPackageDiagram();

    // Re-render to update "used by" labels on other dropdowns
    renderPeripheralView();
}

/** Handle signal name changes in the peripheral view. */
function onPeriphSignalNameChange(e) {
    const input = e.target;
    const signalName = input.dataset.signal;

    // Find the pin this signal is assigned to
    const reverse = buildReverseAssignments();
    const pinPos = reverse[signalName];
    if (pinPos !== undefined) {
        if (input.value.trim()) {
            signalNames[pinPos] = input.value.trim();
        } else {
            delete signalNames[pinPos];
        }

        // Sync signal name to co-assigned peripherals sharing the same pin
        const coAssigned = getAssignmentsAt(pinPos);
        if (coAssigned.length > 1) {
            const val = input.value.trim();
            for (const a of coAssigned) {
                if (a.peripheral === signalName) continue;
                const otherInput = document.querySelector(
                    `.periph-signal-input[data-signal="${a.peripheral}"]`
                );
                if (otherInput) otherInput.value = val;
            }
        }

        renderPackageDiagram();
    }
}

/** Handle fixed-function pin toggle (checkbox) in the peripheral view. */
function onPeriphFixedToggle(e) {
    pushUndo();

    const cb = e.target;
    const signalName = cb.dataset.signal;
    const pinPos = parseInt(cb.dataset.pinPos, 10);
    const direction = cb.dataset.direction;

    if (cb.checked) {
        const pin = deviceData.pins.find(p => p.position === pinPos);
        const newAssign = {
            peripheral: signalName,
            direction: direction,
            ppsval: null,
            rp_number: pin ? pin.rp_number : null,
            fixed: true,
        };

        // Analog inputs can share a pin with other analog functions
        if (isAnalogFunction(signalName)) {
            addAnalogAssignment(pinPos, newAssign);
        } else {
            setAssignment(pinPos, newAssign);
        }

        // Transfer signal name if present
        const input = document.querySelector(`.periph-signal-input[data-signal="${signalName}"]`);
        if (input && input.value.trim()) {
            signalNames[pinPos] = input.value.trim();
        }
    } else {
        removeAssignment(pinPos, signalName);
        // Only clear signal name if no assignments remain on this pin
        if (!assignments[pinPos]) {
            delete signalNames[pinPos];
        }
    }

    updateSummary();
    checkConflicts();
    renderPackageDiagram();

    // Re-render to update badge counts
    renderPeripheralView();
}

/** Update the "Assigned" count in the summary bar. */
function updateSummary() {
    // Count total individual assignments (multi-assignment pins count each peripheral)
    let count = 0;
    for (const val of Object.values(assignments)) {
        count += Array.isArray(val) ? val.length : 1;
    }
    document.getElementById('sum-assigned').textContent = count;
}

/** Sync assigned/unassigned state on existing diagram pin elements (without full re-render). */
function updateDiagramDots() {
    for (const pin of deviceData.pins) {
        const el = document.getElementById(`pkg-pin-${pin.position}`);
        if (el) {
            el.classList.toggle('assigned', !!assignments[pin.position]);
        }
    }
}
