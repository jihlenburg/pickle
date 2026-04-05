/**
 * Reservation and fuse-coupled assignment rules.
 *
 * Owns the frontend logic that dynamically reserves or restores pins when
 * ICSP, JTAG, or alternate I2C routing settings change.
 */
const reservationPolicy = window.PickleReservationPolicy;

// =============================================================================
// ICSP Pin Detection
// =============================================================================

/**
 * Return the currently selected ICSP pair number from the fuse UI (1, 2, or 3).
 * @returns {number}
 */
function getFuseSelect(fieldName) {
    return document.querySelector(`#fuse-fields select[data-field="${fieldName}"]`);
}

function getIcsPair() {
    const el = getFuseSelect('ICS');
    if (!el) return 1;
    const match = el.value.match(/\d+/);
    return match ? parseInt(match[0]) : 1;
}

/** Return true when an I2C alternate pin-routing fuse is enabled for the channel. */
function isI2cAlternateRoutingEnabled(channel) {
    const el = getFuseSelect(`ALTI2C${channel}`);
    return el ? el.value.toUpperCase() === 'ON' : false;
}

/** Check if a function name is one of the fuse-routed I2C pin aliases. */
function isI2cRoutingFunction(fn) {
    return reservationPolicy.isI2cRoutingFunction(fn);
}

/**
 * Parse an I2C pin-routing function name into channel, role, and routing mode.
 * @param {string} fn - e.g. "SCL1" or "ASDA2"
 * @returns {{channel:number, role:string, alternate:boolean}|null}
 */
function parseI2cRoutingFunction(fn) {
    return reservationPolicy.parseI2cRoutingFunction(fn);
}

/** Return whether the given routed I2C function is active for the current fuse state. */
function isI2cRoutingFunctionActive(fn) {
    const parsed = parseI2cRoutingFunction(fn);
    if (!parsed) return true;
    return parsed.alternate === isI2cAlternateRoutingEnabled(parsed.channel);
}

/** Build the routed I2C function name for a channel/role pair. */
function getI2cRoutingFunctionName(channel, role, alternate) {
    return reservationPolicy.getI2cRoutingFunctionName(channel, role, alternate);
}

/** Find the current package pin that carries a given fixed function. */
function findPinByFunction(fn) {
    if (!deviceData) return null;
    return deviceData.pins.find(pin => pin.functions.includes(fn)) || null;
}

/**
 * Return true when a stashed assignment is valid to restore on the given pin.
 * PPS assignments only require an RP-capable pad; fixed assignments must still
 * exist on the current pin and, for I2C routing aliases, be active.
 */
function canRestoreAssignmentOnPin(pin, assignment) {
    if (!pin || !assignment) return false;
    // Handle multi-assignment arrays: check the primary (first) element
    const primary = Array.isArray(assignment) ? assignment[0] : assignment;
    if (!primary) return false;
    if (!primary.fixed) return pin.rp_number !== null;
    if (!pin.functions.includes(primary.peripheral)) return false;
    if (isI2cRoutingFunction(primary.peripheral)) {
        return isI2cRoutingFunctionActive(primary.peripheral);
    }
    return true;
}

/** Stash a pin assignment and its signal name so it can be restored later. */
function stashI2cPinState(pinPos) {
    if (!assignments[pinPos]) return false;
    i2cRoutedAssignments[pinPos] = {
        assignment: structuredClone(assignments[pinPos]),
        signalName: signalNames[pinPos] || '',
    };
    delete assignments[pinPos];
    delete signalNames[pinPos];
    return true;
}

/** Restore any stashed assignments that are valid again for the current routing state. */
function restoreI2cRoutedAssignments() {
    if (!deviceData) return 0;

    let restored = 0;
    for (const [posStr, state] of Object.entries(i2cRoutedAssignments)) {
        const pos = parseInt(posStr, 10);
        if (assignments[pos]) continue;

        const pin = deviceData.pins.find(candidate => candidate.position === pos);
        if (!canRestoreAssignmentOnPin(pin, state.assignment)) continue;

        assignments[pos] = structuredClone(state.assignment);
        if (state.signalName) {
            signalNames[pos] = state.signalName;
        } else {
            delete signalNames[pos];
        }
        delete i2cRoutedAssignments[pos];
        restored += 1;
    }

    return restored;
}

/**
 * Move an assigned I2C function to the active routed pin, preserving signal names.
 * Any assignment already living on the destination pin is stashed first.
 */
function moveI2cAssignment(sourcePos, targetPin, targetFunction) {
    const sourceAssignment = assignments[sourcePos];
    if (!sourceAssignment) return false;

    const sourceSignal = signalNames[sourcePos] || '';
    const targetPos = targetPin.position;

    if (targetPos !== sourcePos && assignments[targetPos]) {
        stashI2cPinState(targetPos);
    }

    assignments[targetPos] = {
        ...structuredClone(sourceAssignment),
        peripheral: targetFunction,
        direction: 'io',
        ppsval: null,
        rp_number: targetPin.rp_number,
        fixed: true,
    };

    if (sourceSignal) {
        signalNames[targetPos] = sourceSignal;
    } else {
        delete signalNames[targetPos];
    }

    if (targetPos !== sourcePos) {
        delete assignments[sourcePos];
        delete signalNames[sourcePos];
    }

    return true;
}

/**
 * Re-route fixed I2C assignments so they follow ALTI2C1/ALTI2C2 automatically.
 * When the selected routing lands on pins that are not bonded out on the current
 * package, the affected assignments are stashed until the route becomes usable.
 */
function reconcileI2cRoutingAssignments() {
    if (!deviceData) {
        return { moved: 0, cleared: 0, restored: 0, missingChannels: [] };
    }

    const routePairs = [
        { channel: 1, role: 'SCL' },
        { channel: 1, role: 'SDA' },
        { channel: 2, role: 'SCL' },
        { channel: 2, role: 'SDA' },
    ];

    let moved = 0;
    let cleared = 0;
    const missingChannels = new Set();

    for (const pair of routePairs) {
        const defaultFunction = getI2cRoutingFunctionName(pair.channel, pair.role, false);
        const alternateFunction = getI2cRoutingFunctionName(pair.channel, pair.role, true);
        const defaultPin = findPinByFunction(defaultFunction);
        const alternatePin = findPinByFunction(alternateFunction);
        const useAlternate = isI2cAlternateRoutingEnabled(pair.channel);
        const activeFunction = useAlternate ? alternateFunction : defaultFunction;
        const activePin = useAlternate ? alternatePin : defaultPin;

        const sourcePin = [defaultPin, alternatePin].find(pin => {
            if (!pin) return false;
            const primary = primaryAssignment(pin.position);
            return primary && (primary.peripheral === defaultFunction || primary.peripheral === alternateFunction);
        });

        if (!sourcePin) continue;

        const sourceAssignment = primaryAssignment(sourcePin.position);
        if (activePin &&
            sourcePin.position === activePin.position &&
            sourceAssignment?.peripheral === activeFunction) {
            continue;
        }

        if (!activePin) {
            if (stashI2cPinState(sourcePin.position)) {
                cleared += 1;
                missingChannels.add(pair.channel);
            }
            continue;
        }

        if (moveI2cAssignment(sourcePin.position, activePin, activeFunction)) {
            moved += 1;
        }
    }

    return {
        moved,
        cleared,
        restored: restoreI2cRoutedAssignments(),
        missingChannels: [...missingChannels].sort(),
    };
}

/** Describe any ALTI2Cx routing that points to pins unavailable on this package. */
function getI2cRoutingWarningForChannel(channel) {
    if (!deviceData || !isI2cAlternateRoutingEnabled(channel)) return '';

    const hasAlternatePins =
        Boolean(findPinByFunction(`ASCL${channel}`)) &&
        Boolean(findPinByFunction(`ASDA${channel}`));
    if (hasAlternatePins) return '';

    return `ASCL${channel}/ASDA${channel} are not exposed on ${deviceData.selected_package}; leave ALTI2C${channel} = OFF for external I2C${channel}.`;
}

/** Sync package-specific warnings into the ALTI2Cx fuse rows. */
function updateFuseFieldWarnings() {
    const warnings = [];

    for (const channel of [1, 2]) {
        const fieldName = `ALTI2C${channel}`;
        const warningText = getI2cRoutingWarningForChannel(channel);
        const row = document.querySelector(`.fuse-row[data-field="${fieldName}"]`);
        const warningEl = row?.querySelector('.fuse-field-warning');

        if (row) row.classList.toggle('warning', Boolean(warningText));
        if (warningEl) {
            warningEl.hidden = !warningText;
            warningEl.textContent = warningText;
        }
        if (warningText) warnings.push(warningText);
    }

    return warnings;
}

/** Return true when the JTAGEN fuse currently reserves the dedicated JTAG pads. */
function isJtagEnabled() {
    const el = getFuseSelect('JTAGEN');
    return el ? el.value.toUpperCase() === 'ON' : false;
}

/**
 * Check if a pin is part of the active ICSP/debug interface.
 * Matches MCLR and the debug pair selected by the ICS fuse setting.
 * @param {Object} pin - Pin object with functions array
 * @returns {boolean}
 */
function isIcspPin(pin) {
    return reservationPolicy.isPinInIcspPair(pin, getIcsPair());
}

/**
 * Check if a function name belongs to the active ICSP debug pair.
 * @param {string} fn - Function name (e.g. "PGC1", "MCLR")
 * @returns {boolean}
 */
function isIcspFunction(fn) {
    return reservationPolicy.isIcspFunctionForPair(fn, getIcsPair());
}

/** Check if a function belongs to the fixed JTAG interface. */
function isJtagFunction(fn) {
    return reservationPolicy.isJtagFunction(fn);
}

/** Return the JTAG role carried by this pin, if any. */
function getJtagFunction(pin) {
    return pin.functions.find(isJtagFunction) || null;
}

/** Check if a pin is currently reserved by the JTAG module. */
function isJtagPin(pin) {
    return isJtagEnabled() && Boolean(getJtagFunction(pin));
}

/**
 * Drop user assignments from pins that are currently reserved by JTAG.
 * This keeps code generation from emitting GPIO/PPS setup for pins the debug
 * module owns while JTAGEN is enabled.
 * @returns {number} Number of assignments removed
 */
function releaseReservedJtagAssignments() {
    if (!deviceData || !isJtagEnabled()) return 0;

    let cleared = 0;
    for (const pin of deviceData.pins) {
        if (!isJtagPin(pin)) continue;
        if (assignments[pin.position]) {
            jtagReservedAssignments[pin.position] = structuredClone(assignments[pin.position]);
            delete assignments[pin.position];
            cleared += 1;
        }
    }
    return cleared;
}

/**
 * Restore assignments that were auto-cleared when JTAG took ownership of its pins.
 * Runs only when JTAGEN is OFF again.
 * @returns {number} Number of assignments restored
 */
function restoreJtagAssignments() {
    if (!deviceData || isJtagEnabled()) return 0;

    let restored = 0;
    for (const [posStr, assignment] of Object.entries(jtagReservedAssignments)) {
        const pos = parseInt(posStr, 10);
        if (!assignments[pos]) {
            assignments[pos] = structuredClone(assignment);
            restored += 1;
        }
    }
    jtagReservedAssignments = {};
    return restored;
}

/** Apply fuse-driven pin reservations and re-render the current device view. */
function applyFuseReservations() {
    const i2cRouting = reconcileI2cRoutingAssignments();
    if (isJtagEnabled()) {
        releaseReservedJtagAssignments();
    } else {
        restoreJtagAssignments();
    }
    if (deviceData) {
        renderCurrentEditorView();
    }

    const warnings = updateFuseFieldWarnings();

    if (i2cRouting.moved || i2cRouting.cleared || i2cRouting.restored) {
        setStatus(
            warnings.length > 0
                ? 'I2C routing updated; see ALTI2C warnings'
                : 'I2C pins reallocated for the current ALTI2C fuse settings'
        );
    }
}

/**
 * Refresh ICSP gold highlighting on pin table rows, function tags,
 * and package diagram pins to match the current ICS fuse pair.
 */
function refreshIcspHighlight() {
    if (!deviceData) return;
    for (const pin of deviceData.pins) {
        const isIcsp = isIcspPin(pin);
        const isJtag = isJtagPin(pin);

        // Pin table row
        const row = document.getElementById(`pin-row-${pin.position}`);
        if (row) row.classList.toggle('icsp', isIcsp);
        if (row) row.classList.toggle('jtag', isJtag);

        // Package diagram pin
        const pkgPin = document.getElementById(`pkg-pin-${pin.position}`);
        if (pkgPin) pkgPin.classList.toggle('icsp', isIcsp);
        if (pkgPin) pkgPin.classList.toggle('jtag', isJtag);
    }
    // Function tags — check each individually
    document.querySelectorAll('.func-tag').forEach(span => {
        span.classList.toggle('icsp', isIcspFunction(span.textContent));
        span.classList.toggle('jtag', isJtagFunction(span.textContent) && isJtagEnabled());
    });
}

// =============================================================================
// Conflict Detection
// =============================================================================

function collectAssignmentConflicts() {
    return reservationPolicy.analyzeAssignmentConflicts(assignments, {
        isAnalogFunction,
        isAnalogOutput,
    });
}

function applyConflictHighlights(conflictPins) {
    if (!deviceData) return;
    for (const pin of deviceData.pins) {
        const pkgEl = document.getElementById(`pkg-pin-${pin.position}`);
        const rowEl = document.getElementById(`pin-row-${pin.position}`);
        const isConflict = conflictPins.has(pin.position);
        if (pkgEl) pkgEl.classList.toggle('conflict', isConflict);
        if (rowEl) rowEl.classList.toggle('conflict', isConflict);
    }
}

/**
 * Detect duplicate peripheral assignments and highlight conflicts on the
 * package diagram and pin table.
 * @returns {Set<number>} Set of conflicting pin positions
 */
function checkConflicts() {
    const box = $('conflict-box');
    const { messages, conflictPins } = collectAssignmentConflicts();
    box.textContent = messages.join('\n');
    applyConflictHighlights(conflictPins);

    return conflictPins;
}
