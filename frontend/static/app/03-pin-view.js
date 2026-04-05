/**
 * Pin-table and package-diagram rendering.
 *
 * Owns the physical-pin-oriented view, including the package SVG/grid output
 * and the row-level controls shown for each resolved pin.
 */
// Ordered browser scripts share one global scope, so keep file-local aliases
// unique instead of redeclaring the same top-level `const` in multiple files.
const pinViewModel = window.PickleViewModel;

// =============================================================================
// Pin Table Rendering
// =============================================================================

/** Render the full pin table and package diagram from current deviceData and assignments. */
function renderDevice() {
    const tbody = $('pin-tbody');
    tbody.innerHTML = '';

    releaseReservedJtagAssignments();

    renderLeftPanelViewFrame();

    for (const pin of deviceData.pins) {
        const presentation = pinViewModel.buildPinPresentation(pin, {
            signalNames,
            getAssignmentsAt,
            isIcspPin,
            isJtagPin,
            getJtagFunction,
        });
        const tr = document.createElement('tr');
        tr.className = 'pin-row';
        tr.id = `pin-row-${pin.position}`;
        if (pin.is_power) tr.classList.add('power');
        if (presentation.icsp) tr.classList.add('icsp');
        if (presentation.jtag) tr.classList.add('jtag');
        if (presentation.assigned) tr.classList.add('assigned');

        // Column: pin number
        const tdNum = document.createElement('td');
        tdNum.className = 'pin-num';
        tdNum.textContent = pin.position;
        tr.appendChild(tdNum);

        // Column: pin name (port label or pad name)
        const tdName = document.createElement('td');
        tdName.className = 'pin-name';
        tdName.textContent = presentation.portLabel;
        tr.appendChild(tdName);

        // Column: available functions (as clickable colored tags)
        const tdFunc = document.createElement('td');
        const funcDiv = document.createElement('div');
        funcDiv.className = 'pin-functions';

        const isPower = pin.is_power;
        const isIcsp = presentation.icsp;
        const isJtag = presentation.jtag;
        const currentAssigns = presentation.assignmentsAt;
        const hasNonFixed = currentAssigns.some(a => !a.fixed);

        for (const fn of pin.functions) {
            if (isI2cRoutingFunction(fn) && !isI2cRoutingFunctionActive(fn)) continue;

            const span = document.createElement('span');
            const colorClass = funcTagColorClass(fn);
            const isRp = /^RP\d+$/.test(fn);

            span.className = `func-tag ${colorClass}`;

            // Determine clickability: power and active ICSP/JTAG functions are reserved
            const isReservedIcsp = isIcspFunction(fn);
            const isReservedJtag = isJtagFunction(fn) && isJtagEnabled();

            if (isPower || isReservedIcsp || isReservedJtag) {
                span.classList.add('reserved');
            } else {
                span.classList.add('clickable');
                span.dataset.pinPos = pin.position;
                span.dataset.function = fn;
                span.addEventListener('click', onFuncTagClick);

                // Highlight selected state
                if (isRp) {
                    if (hasNonFixed) span.classList.add('selected');
                } else if (hasAssignmentFor(pin.position, fn)) {
                    span.classList.add('selected');
                }
            }

            // RP tags display as "PPS" with the actual RP number in tooltip
            if (isRp) {
                span.textContent = 'PPS';
                span.title = `${fn} — click to select a remappable peripheral`;
            } else {
                span.textContent = fn;
                const desc = getDescription(fn);
                if (desc) span.title = desc;
            }

            funcDiv.appendChild(span);
        }
        tdFunc.appendChild(funcDiv);
        tr.appendChild(tdFunc);

        // Column: Remapping (PPS-only dropdown for RP-capable pins)
        const tdRemap = document.createElement('td');
        if (!isPower) {
            if (isJtag) {
                const reserved = document.createElement('span');
                reserved.className = 'pin-reserved';
                reserved.textContent = 'JTAG';
                reserved.title = `${getJtagFunction(pin) || 'JTAG'} reserved while JTAGEN = ON`;
                tdRemap.appendChild(reserved);
            } else if (pin.rp_number !== null) {
                const periphs = getAvailablePeripherals(pin);
                const ppsPeriphs = periphs.filter(p => !p.fixed);

                if (ppsPeriphs.length > 0) {
                    const select = document.createElement('select');
                    select.className = 'remap-select';
                    select.dataset.pinPos = pin.position;
                    select.dataset.rpNum = pin.rp_number;

                    const optNone = document.createElement('option');
                    optNone.value = '';
                    optNone.textContent = `\u2014 RP${pin.rp_number} \u2014`;
                    select.appendChild(optNone);

                    // Build optgroups by peripheral group
                    const seen = new Set();
                    for (const p of ppsPeriphs) {
                        if (seen.has(p.group)) continue;
                        seen.add(p.group);

                        const optgroup = document.createElement('optgroup');
                        optgroup.label = p.group;
                        const groupItems = ppsPeriphs.filter(x => x.group === p.group);
                        for (const gp of groupItems) {
                            const opt = document.createElement('option');
                            const dirLabel = gp.direction === 'out' ? 'OUT' : 'IN';
                            opt.value = JSON.stringify({
                                name: gp.name, direction: gp.direction,
                                ppsval: gp.ppsval,
                            });
                            opt.textContent = `${gp.name} (${dirLabel})`;
                            opt.className = periphClass(gp.name);
                            const optDesc = getDescription(gp.name);
                            if (optDesc) opt.title = optDesc;
                            optgroup.appendChild(opt);
                        }
                        select.appendChild(optgroup);
                    }

                    // Restore previous PPS assignment if present
                    const ppsAssign = currentAssigns.find(a => !a.fixed);
                    if (ppsAssign) {
                        const val = JSON.stringify({
                            name: ppsAssign.peripheral,
                            direction: ppsAssign.direction,
                            ppsval: ppsAssign.ppsval,
                        });
                        select.value = val;
                    }

                    select.addEventListener('change', onRemapChange);
                    tdRemap.appendChild(select);
                }
            }
        }
        tr.appendChild(tdRemap);

        // Column: signal name input (non-power pins only)
        const tdSig = document.createElement('td');
        if (!pin.is_power) {
            const input = document.createElement('input');
            input.type = 'text';
            input.className = 'signal-input';
            input.placeholder = 'e.g. UART1_TX';
            input.dataset.pinPos = pin.position;
            if (presentation.signalName) {
                input.value = presentation.signalName;
            }
            if (presentation.jtag) {
                input.disabled = true;
                input.title = 'Signal names are disabled while the pin is reserved by JTAG';
            }
            // Push undo snapshot when field gains focus (before user types)
            input.addEventListener('focus', () => pushUndo());
            input.addEventListener('input', (e) => {
                applySignalNameMutation(() => {
                    const pos = parseInt(e.target.dataset.pinPos, 10);
                    if (e.target.value.trim()) {
                        signalNames[pos] = e.target.value.trim();
                    } else {
                        delete signalNames[pos];
                    }
                });
            });
            tdSig.appendChild(input);
        }
        tr.appendChild(tdSig);

        tbody.appendChild(tr);
    }

    finalizeLeftPanelViewFrame();
}

// =============================================================================
// Package Diagram
// =============================================================================

/**
 * Get the display label for a pin in the package diagram.
 * Shows signal name or peripheral if assigned, otherwise a reserved JTAG role
 * or the default port/pad name.
 */
function pinLabel(pin) {
    return pinViewModel.buildPinPresentation(pin, {
        signalNames,
        getAssignmentsAt,
        isIcspPin,
        isJtagPin,
        getJtagFunction,
    }).packageLabel;
}

/** Check if the current package uses QFN/QFP layout (vs DIP/SSOP). */
function isQfnPackage() {
    return /QFN|QFP|TQFP/i.test(deviceData.selected_package);
}

/** Render the package diagram into the container, choosing DIP or QFN layout. */
function renderPackageDiagram() {
    const container = document.getElementById('pkg-container');
    container.innerHTML = '';

    if (isQfnPackage()) {
        renderQfnDiagram(container);
    } else {
        renderDipDiagram(container);
    }
}

// Re-evaluate diagram orientation when window resizes
window.addEventListener('resize', (() => {
    let timer;
    return () => {
        clearTimeout(timer);
        timer = setTimeout(() => { if (deviceData) renderPackageDiagram(); }, appConfig.ui.timings.packageDiagramDebounceMs);
    };
})());

/**
 * Create a single pin element for the package diagram.
 * @param {Object} pin - Pin data object
 * @param {string} side - Position: 'left', 'right', 'top', or 'bottom'
 * @returns {HTMLElement}
 */
function makePinEl(pin, side) {
    const presentation = pinViewModel.buildPinPresentation(pin, {
        signalNames,
        getAssignmentsAt,
        isIcspPin,
        isJtagPin,
        getJtagFunction,
    });
    const el = document.createElement('div');
    el.className = `pkg-pin pkg-pin-${side}`;
    el.id = `pkg-pin-${pin.position}`;
    el.onclick = () => scrollToPin(pin.position);
    if (pin.is_power) el.classList.add('power');
    if (presentation.icsp) el.classList.add('icsp');
    if (presentation.jtag) el.classList.add('jtag');
    if (presentation.assigned) el.classList.add('assigned');

    const name = presentation.packageLabel;
    el.title = `${pin.position}: ${name}`;

    if (side === 'left') {
        const lbl = document.createElement('span');
        lbl.className = 'pkg-pin-label';
        lbl.textContent = `${pin.position} ${name}`;
        el.appendChild(lbl);
    } else if (side === 'right') {
        const lbl = document.createElement('span');
        lbl.className = 'pkg-pin-label';
        lbl.textContent = `${name} ${pin.position}`;
        el.appendChild(lbl);
    } else if (side === 'top') {
        const num = document.createElement('span');
        num.className = 'pkg-pin-num';
        num.textContent = pin.position;
        const lbl = document.createElement('span');
        lbl.className = 'pkg-pin-label';
        lbl.textContent = name;
        el.appendChild(num);
        el.appendChild(lbl);
    } else {
        const lbl = document.createElement('span');
        lbl.className = 'pkg-pin-label';
        lbl.textContent = name;
        const num = document.createElement('span');
        num.className = 'pkg-pin-num';
        num.textContent = pin.position;
        el.appendChild(lbl);
        el.appendChild(num);
    }
    return el;
}

/** Render a DIP/SSOP-style package diagram (two rows of pins facing each other).
 *  Automatically flips to a horizontal (90° rotated) layout when the vertical
 *  diagram would exceed 30% of the available panel height. */
function renderDipDiagram(container) {
    const pins = deviceData.pins;
    const half = Math.ceil(pins.length / 2);

    // ~18px per pin row + ~40px overhead — flip when vertical would exceed 50% of panel
    const estimatedHeight = half * 18 + 40;
    const panel = container.closest('.panel-left');
    const availableHeight = panel ? panel.clientHeight : window.innerHeight;

    if (estimatedHeight > availableHeight * 0.5) {
        renderDipDiagramHorizontal(container, pins, half);
    } else {
        renderDipDiagramVertical(container, pins, half);
    }
}

/** Standard vertical DIP layout — pins on left/right, outside the chip body. */
function renderDipDiagramVertical(container, pins, half) {
    const leftPins = pins.slice(0, half);
    const rightPins = pins.slice(half).reverse();

    const diagram = document.createElement('div');
    diagram.className = 'pkg-diagram';

    const wrapper = document.createElement('div');
    wrapper.className = 'chip-dip-vt';

    // Left pin column
    const leftCol = document.createElement('div');
    leftCol.className = 'chip-dip-vt-pins chip-dip-vt-left';
    leftPins.forEach(pin => leftCol.appendChild(makePinEl(pin, 'left')));
    wrapper.appendChild(leftCol);

    // Chip body with labels
    const body = document.createElement('div');
    body.className = 'chip-dip-vt-body';

    const notch = document.createElement('div');
    notch.className = 'notch';
    body.appendChild(notch);

    const label = document.createElement('div');
    label.className = 'chip-label';
    label.textContent = deviceData.part_number;
    body.appendChild(label);

    const sublabel = document.createElement('div');
    sublabel.className = 'chip-sublabel';
    sublabel.textContent = deviceData.selected_package;
    body.appendChild(sublabel);

    wrapper.appendChild(body);

    // Right pin column
    const rightCol = document.createElement('div');
    rightCol.className = 'chip-dip-vt-pins chip-dip-vt-right';
    rightPins.forEach(pin => rightCol.appendChild(makePinEl(pin, 'right')));
    wrapper.appendChild(rightCol);

    diagram.appendChild(wrapper);
    container.appendChild(diagram);
}

/** Horizontal DIP layout — chip rotated 90° clockwise.
 *  Top row (left→right): pin 14..1, bottom row (left→right): pin 15..28.
 *  Notch on the right (was at top in vertical). Pin 1 is top-right. */
function renderDipDiagramHorizontal(container, pins, half) {
    const topPins = pins.slice(0, half).reverse();
    const bottomPins = pins.slice(half);

    const diagram = document.createElement('div');
    diagram.className = 'pkg-diagram';

    const wrapper = document.createElement('div');
    wrapper.className = 'chip-dip-hz';

    // Top pin row
    const topRow = document.createElement('div');
    topRow.className = 'chip-dip-hz-pins chip-dip-hz-top';
    topPins.forEach(pin => topRow.appendChild(makePinEl(pin, 'top')));
    wrapper.appendChild(topRow);

    // Chip body with labels
    const body = document.createElement('div');
    body.className = 'chip-dip-hz-body';

    const notch = document.createElement('div');
    notch.className = 'chip-dip-hz-notch';
    body.appendChild(notch);

    const label = document.createElement('div');
    label.className = 'chip-label';
    label.textContent = deviceData.part_number;
    body.appendChild(label);

    const sublabel = document.createElement('div');
    sublabel.className = 'chip-sublabel';
    sublabel.textContent = deviceData.selected_package;
    body.appendChild(sublabel);

    wrapper.appendChild(body);

    // Bottom pin row
    const bottomRow = document.createElement('div');
    bottomRow.className = 'chip-dip-hz-pins chip-dip-hz-bottom';
    bottomPins.forEach(pin => bottomRow.appendChild(makePinEl(pin, 'bottom')));
    wrapper.appendChild(bottomRow);

    diagram.appendChild(wrapper);
    container.appendChild(diagram);
}

/** Render a QFN/QFP/TQFP-style package diagram (pins on all four sides). */
function renderQfnDiagram(container) {
    const pins = deviceData.pins;
    const n = pins.length;
    const side = Math.ceil(n / 4);

    const leftPins = pins.slice(0, side);
    const bottomPins = pins.slice(side, side * 2);
    const rightPins = pins.slice(side * 2, side * 3).reverse();
    const topPins = pins.slice(side * 3).reverse();

    const wrapper = document.createElement('div');
    wrapper.className = 'pkg-diagram';

    // Compute a square body: fixed size, rows and columns sized to match
    const bodySize = Math.max(side * 26, 160);
    const rowH = Math.floor(bodySize / leftPins.length);
    const colW = Math.floor(bodySize / bottomPins.length);

    const grid = document.createElement('div');
    grid.className = 'qfn-grid';
    grid.style.gridTemplateColumns = `auto repeat(${bottomPins.length}, ${colW}px) auto`;
    grid.style.gridTemplateRows = `auto repeat(${leftPins.length}, ${rowH}px) auto`;

    // Pin 1 marker (top-left corner)
    const marker = document.createElement('div');
    marker.className = 'qfn-corner qfn-marker';
    marker.innerHTML = '&#x25CF;';
    grid.appendChild(marker);

    // Top row pins
    for (const pin of topPins) {
        grid.appendChild(makePinEl(pin, 'top'));
    }
    for (let i = topPins.length; i < bottomPins.length; i++) {
        grid.appendChild(document.createElement('div'));
    }
    grid.appendChild(document.createElement('div'));

    // Middle rows: left pin | chip body | right pin
    for (let i = 0; i < leftPins.length; i++) {
        grid.appendChild(makePinEl(leftPins[i], 'left'));
        if (i === 0) {
            const body = document.createElement('div');
            body.className = 'qfn-body';
            body.style.gridColumn = `2 / ${bottomPins.length + 2}`;
            body.style.gridRow = `2 / ${leftPins.length + 2}`;
            body.style.width = `${bodySize}px`;
            body.style.height = `${bodySize}px`;
            body.innerHTML = `<div class="chip-label">${deviceData.part_number}</div>
                              <div class="chip-sublabel">${deviceData.selected_package}</div>`;
            grid.appendChild(body);
        }
        if (rightPins[i]) grid.appendChild(makePinEl(rightPins[i], 'right'));
        else grid.appendChild(document.createElement('div'));
    }

    // Bottom row
    grid.appendChild(document.createElement('div'));
    for (const pin of bottomPins) {
        grid.appendChild(makePinEl(pin, 'bottom'));
    }
    grid.appendChild(document.createElement('div'));

    wrapper.appendChild(grid);
    container.appendChild(wrapper);
}

/**
 * Scroll the pin table to center a specific pin row and briefly highlight it.
 * Uses getBoundingClientRect for reliable positioning in nested scroll containers.
 * @param {number} pos - Pin position number
 */
function scrollToPin(pos) {
    // If in peripheral view, switch to pin view first
    if (activeView === 'peripheral') {
        switchView('pin');
    }
    const row = document.getElementById(`pin-row-${pos}`);
    if (row) {
        const container = document.getElementById('pin-view-container');
        if (container) {
            const containerRect = container.getBoundingClientRect();
            const rowRect = row.getBoundingClientRect();
            const offset = rowRect.top - containerRect.top + container.scrollTop;
            container.scrollTo({ top: offset - container.clientHeight / 2, behavior: 'smooth' });
        } else {
            row.scrollIntoView({ behavior: 'smooth', block: 'center' });
        }
        row.style.background = 'var(--pin-scroll-highlight)';
        setTimeout(() => { row.style.background = ''; }, appConfig.ui.timings.pinScrollHighlightMs);
    }
}
