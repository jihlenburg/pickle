/**
 * Peripheral-centric data shaping and rendering.
 *
 * Transforms the raw pin/device model into grouped peripheral instances and
 * renders the expandable peripheral assignment view.
 */
// =============================================================================
// Peripheral-Centric View — Data Layer
// =============================================================================

/** @type {'pin'|'peripheral'} Currently active left-panel view */
let activeView = 'pin';

/** @type {Map<string, boolean>} Tracks which peripheral cards are expanded */
let periphCardState = {};

/**
 * Extract the peripheral instance identity from a signal name.
 * @param {string} name - Signal name (e.g. "U1TX", "SDI2", "PWM3H")
 * @returns {{type:string, instance:string, id:string}|null}
 */
function extractPeripheralInstance(name) {
    let m;
    if ((m = name.match(/^U(\d+)/))) return { type: 'UART', instance: m[1], id: `UART${m[1]}` };
    if ((m = name.match(/^(?:SDI|SDO|SCK|SS)(\d+)/))) return { type: 'SPI', instance: m[1], id: `SPI${m[1]}` };
    if ((m = name.match(/^A?(?:SCL|SDA)(\d+)$/))) return { type: 'I2C', instance: m[1], id: `I2C${m[1]}` };
    if ((m = name.match(/^C(\d+)(?:TX|RX)$/))) return { type: 'CAN', instance: m[1], id: `CAN${m[1]}` };
    if ((m = name.match(/^SENT(\d+)/))) return { type: 'SENT', instance: m[1], id: `SENT${m[1]}` };
    if ((m = name.match(/^PWM(\d+)[HL]$/))) return { type: 'PWM', instance: m[1], id: `PWM${m[1]}` };
    if ((m = name.match(/^(?:QE[AB]|HOME|INDX)(\d+)$/))) return { type: 'QEI', instance: m[1], id: `QEI${m[1]}` };
    if ((m = name.match(/^QEICCMP(\d+)$/))) return { type: 'QEI', instance: m[1], id: `QEI${m[1]}` };
    if ((m = name.match(/^T(\d+)CK$/))) return { type: 'Timer', instance: m[1], id: `Timer${m[1]}` };
    if ((m = name.match(/^TCKI(\d+)$/))) return { type: 'Timer', instance: m[1], id: `Timer${m[1]}` };
    if ((m = name.match(/^ICM(\d+)$/))) return { type: 'Input Capture', instance: m[1], id: `ICM${m[1]}` };
    if ((m = name.match(/^OCM(\d+)/))) return { type: 'CCP', instance: m[1], id: `CCP${m[1]}` };
    if ((m = name.match(/^CMP(\d+)[A-D]?$/))) return { type: 'Comparator', instance: m[1], id: `CMP${m[1]}` };
    if ((m = name.match(/^CLC(\d+)/))) return { type: 'CLC', instance: m[1], id: `CLC${m[1]}` };
    if ((m = name.match(/^INT(\d+)$/))) return { type: 'Interrupt', instance: m[1], id: `INT${m[1]}` };
    if ((m = name.match(/^PCI(\d+)$/))) return { type: 'PWM Fault', instance: m[1], id: `PCI${m[1]}` };
    if ((m = name.match(/^ADCTRG(\d+)$/))) return { type: 'ADC Trigger', instance: m[1], id: `ADCTRG${m[1]}` };
    if ((m = name.match(/^PTGTRG(\d+)$/))) return { type: 'PTG', instance: m[1], id: `PTG${m[1]}` };
    if ((m = name.match(/^OA(\d+)/))) return { type: 'Op-Amp', instance: m[1], id: `OA${m[1]}` };
    // ADC channels — shared (AN#) grouped together, dedicated (ANA#) as individual cores
    if ((m = name.match(/^ANA(\d+)$/))) return { type: 'ADC', instance: String(parseInt(m[1]) + 1), id: `ADC${m[1]}`, label: `ADC${m[1]} (dedicated)` };
    if ((m = name.match(/^AN(\d+)$/))) return { type: 'ADC', instance: '0', id: 'ADC', label: 'ADC (shared)' };
    // DAC
    if (/^DAC\d*OUT$/.test(name)) return { type: 'DAC', instance: '0', id: 'DAC' };
    // Bias current
    if ((m = name.match(/^IBIAS(\d+)$/))) return { type: 'Bias', instance: '0', id: 'Bias Current' };
    // Singletons and shared signals — group by periphGroupFine classification
    if (/^PWME[A-Z]$/.test(name)) return { type: 'PWM Event', instance: '0', id: 'PWM Events' };
    if (/^PWMTRG/.test(name)) return { type: 'PWM Trigger', instance: '0', id: 'PWM Triggers' };
    if (/^OCF[A-Z]$/.test(name)) return { type: 'CCP Fault', instance: '0', id: 'CCP Faults' };
    if (/^CLCIN/.test(name)) return { type: 'CLC Input', instance: '0', id: 'CLC Inputs' };
    if (/^REF[IO]$/.test(name)) return { type: 'Reference Clock', instance: '0', id: 'Ref Clock' };
    if (/^RPV\d/.test(name)) return { type: 'Virtual Pin', instance: '0', id: 'Virtual Pins' };
    return null;
}

/** Category ordering for peripheral instance groups. */
const PERIPH_CATEGORIES = [
    { name: 'Communication', types: ['UART', 'SPI', 'I2C', 'CAN', 'SENT'] },
    { name: 'Motor Control', types: ['PWM', 'PWM Event', 'PWM Trigger', 'QEI'] },
    { name: 'Timing', types: ['Timer', 'Input Capture', 'CCP', 'CCP Fault'] },
    { name: 'Analog', types: ['ADC', 'Comparator', 'ADC Trigger', 'Op-Amp', 'DAC', 'Bias'] },
    { name: 'Logic', types: ['CLC', 'CLC Input'] },
    { name: 'System', types: ['Interrupt', 'PWM Fault', 'PTG', 'Reference Clock', 'Virtual Pin'] },
];

/**
 * Build an ordered list of peripheral instances from current deviceData.
 * Combines PPS remappable signals and fixed functions into per-instance groups.
 * @returns {Array<{id:string, type:string, instance:string, category:string, signals:Array}>}
 */
function buildPeripheralInstances() {
    if (!deviceData) return [];

    const instanceMap = {};  // id -> instance object

    const getOrCreate = (info, category) => {
        if (!instanceMap[info.id]) {
            instanceMap[info.id] = {
                id: info.id,
                label: info.label || info.id,
                type: info.type,
                instance: info.instance,
                category: category || 'Miscellaneous',
                signals: [],
            };
        }
        return instanceMap[info.id];
    };

    const findCategory = (type) => {
        for (const cat of PERIPH_CATEGORIES) {
            if (cat.types.includes(type)) return cat.name;
        }
        return 'Miscellaneous';
    };

    // Collect PPS remappable inputs
    for (const inp of deviceData.remappable_inputs) {
        const info = extractPeripheralInstance(inp.name);
        if (!info) continue;
        const inst = getOrCreate(info, findCategory(info.type));
        inst.signals.push({
            name: inp.name,
            direction: 'in',
            ppsval: null,
            fixed: false,
            fixedPin: null,
        });
    }

    // Collect PPS remappable outputs
    for (const out of deviceData.remappable_outputs) {
        const info = extractPeripheralInstance(out.name);
        if (!info) continue;
        const inst = getOrCreate(info, findCategory(info.type));
        inst.signals.push({
            name: out.name,
            direction: 'out',
            ppsval: out.ppsval,
            fixed: false,
            fixedPin: null,
        });
    }

    // Collect fixed functions from pins (I2C SDA/SCL, PWM H/L, comparators, op-amps, etc.)
    for (const pin of deviceData.pins) {
        if (pin.is_power) continue;
        for (const fn of pin.functions) {
            if (isI2cRoutingFunction(fn) && !isI2cRoutingFunctionActive(fn)) continue;
            const info = extractPeripheralInstance(fn);
            if (!info) continue;
            const inst = getOrCreate(info, findCategory(info.type));
            // Only add as fixed if not already present as a PPS signal
            const alreadyPps = inst.signals.some(s => s.name === fn && !s.fixed);
            if (alreadyPps) continue;
            // Avoid duplicate fixed entries
            if (inst.signals.some(s => s.name === fn && s.fixed)) continue;
            inst.signals.push({
                name: fn,
                direction: fixedFuncDirection(fn),
                ppsval: null,
                fixed: true,
                fixedPin: pin.position,
            });
        }
    }

    // Sort signals within each instance: outputs first, then inputs, then by natural name order
    const naturalCmp = (a, b) => a.localeCompare(b, undefined, { numeric: true, sensitivity: 'base' });
    for (const inst of Object.values(instanceMap)) {
        inst.signals.sort((a, b) => {
            if (a.direction !== b.direction) {
                const order = { out: 0, io: 1, in: 2 };
                return (order[a.direction] ?? 3) - (order[b.direction] ?? 3);
            }
            return naturalCmp(a.name, b.name);
        });
    }

    // Sort instances by category order, then type, then instance number
    const catOrder = {};
    PERIPH_CATEGORIES.forEach((c, i) => { catOrder[c.name] = i; });
    catOrder['Miscellaneous'] = PERIPH_CATEGORIES.length;

    return Object.values(instanceMap).sort((a, b) => {
        const co = (catOrder[a.category] ?? 99) - (catOrder[b.category] ?? 99);
        if (co !== 0) return co;
        const tc = a.type.localeCompare(b.type);
        if (tc !== 0) return tc;
        return parseInt(a.instance) - parseInt(b.instance);
    });
}

/**
 * Build a reverse lookup from assignments: peripheral name -> pin position.
 * @returns {Object.<string, number>}
 */
function buildReverseAssignments() {
    return window.PickleModel.buildReverseAssignments(assignments);
}

/**
 * Get all RP-capable pins available for a PPS signal assignment.
 * @param {string} signalName - The peripheral signal (e.g. "U1TX")
 * @param {string} signalDirection - "in" or "out"
 * @returns {Array<{pin:Object, label:string, usedBy:string|null}>}
 */
function getAvailableRpPins(signalName, signalDirection) {
    if (!deviceData) return [];

    const results = [];
    for (const pin of deviceData.pins) {
        if (pin.rp_number === null || pin.is_power) continue;
        if (isIcspPin(pin)) continue;
        if (isJtagPin(pin)) continue;

        const portName = pin.port ? `R${pin.port}${pin.port_bit}` : pin.pad_name;
        const label = `Pin ${pin.position} — ${portName} (RP${pin.rp_number})`;

        let usedBy = null;
        const existingList = getAssignmentsAt(pin.position);
        const otherPeriphs = existingList.filter(a => a.peripheral !== signalName);
        if (otherPeriphs.length > 0) {
            usedBy = otherPeriphs.map(a => a.peripheral).join(', ');
        }

        results.push({ pin, label, usedBy });
    }

    return results;
}

// =============================================================================
// Peripheral View Rendering
// =============================================================================

/** Render the full peripheral-centric view into #periph-view. */
function renderPeripheralView() {
    const container = $('periph-view');
    container.innerHTML = '';

    if (!deviceData) return;

    renderDeviceSummary();

    renderPackageDiagram();

    const instances = buildPeripheralInstances();
    const reverse = buildReverseAssignments();

    // Toolbar: expand/collapse all
    const toolbar = document.createElement('div');
    toolbar.className = 'periph-toolbar';
    const expandBtn = document.createElement('button');
    expandBtn.className = 'periph-toolbar-btn';
    expandBtn.textContent = 'Expand All';
    expandBtn.addEventListener('click', () => {
        container.querySelectorAll('.periph-card').forEach(c => {
            c.classList.add('expanded');
            periphCardState[c.dataset.id] = true;
        });
    });
    const collapseBtn = document.createElement('button');
    collapseBtn.className = 'periph-toolbar-btn';
    collapseBtn.textContent = 'Collapse All';
    collapseBtn.addEventListener('click', () => {
        container.querySelectorAll('.periph-card').forEach(c => {
            c.classList.remove('expanded');
            periphCardState[c.dataset.id] = false;
        });
    });
    toolbar.appendChild(expandBtn);
    toolbar.appendChild(collapseBtn);
    container.appendChild(toolbar);

    // Group instances by category and render
    let currentCategory = '';
    for (const inst of instances) {
        if (inst.category !== currentCategory) {
            currentCategory = inst.category;
            const heading = document.createElement('div');
            heading.className = 'periph-section-heading';
            heading.textContent = currentCategory;
            container.appendChild(heading);
        }
        container.appendChild(renderPeriphCard(inst, reverse));
    }

    updateSummary();
    checkConflicts();
}

/**
 * Render a single peripheral instance card.
 * @param {Object} inst - Instance from buildPeripheralInstances()
 * @param {Object} reverse - Reverse assignment map from buildReverseAssignments()
 * @returns {HTMLElement}
 */
function renderPeriphCard(inst, reverse) {
    const card = document.createElement('div');
    card.className = 'periph-card';
    card.dataset.id = inst.id;

    // Count assigned signals
    const assignedCount = inst.signals.filter(s => reverse[s.name] !== undefined).length;
    const hasAssignments = assignedCount > 0;

    if (hasAssignments) card.classList.add('has-assignments');

    // Auto-expand: if user has toggled this card before, use that state;
    // otherwise expand cards that have assignments
    const remembered = periphCardState[inst.id];
    const shouldExpand = remembered !== undefined ? remembered : hasAssignments;
    if (shouldExpand) card.classList.add('expanded');

    // Determine if all signals are fixed
    const allFixed = inst.signals.every(s => s.fixed);

    // Header
    const header = document.createElement('div');
    header.className = 'periph-card-header';
    header.addEventListener('click', () => {
        card.classList.toggle('expanded');
        periphCardState[inst.id] = card.classList.contains('expanded');
    });

    const chevron = document.createElement('span');
    chevron.className = 'periph-card-chevron';
    chevron.textContent = '\u25B6';
    header.appendChild(chevron);

    const title = document.createElement('span');
    title.className = 'periph-card-title';
    title.textContent = inst.label;
    header.appendChild(title);

    if (allFixed) {
        const fixedTag = document.createElement('span');
        fixedTag.className = 'periph-card-fixed-tag';
        fixedTag.textContent = 'fixed';
        header.appendChild(fixedTag);
    }

    const badge = document.createElement('span');
    badge.className = 'periph-card-badge';
    badge.textContent = `${assignedCount}/${inst.signals.length}`;
    header.appendChild(badge);

    card.appendChild(header);

    // Body
    const body = document.createElement('div');
    body.className = 'periph-card-body';

    const table = document.createElement('table');
    table.className = 'periph-signal-table';

    const thead = document.createElement('thead');
    thead.innerHTML = '<tr><th>Signal</th><th>Pin Assignment</th><th>Signal Name</th></tr>';
    table.appendChild(thead);

    const tbody = document.createElement('tbody');
    for (const signal of inst.signals) {
        const tr = document.createElement('tr');
        tr.className = 'periph-signal-row';

        // Signal name column
        const tdName = document.createElement('td');
        const nameSpan = document.createElement('span');
        nameSpan.className = `periph-signal-name ${periphClass(signal.name)}`;
        nameSpan.textContent = signal.name;
        const desc = getDescription(signal.name);
        if (desc) nameSpan.title = desc;
        tdName.appendChild(nameSpan);

        const dirSpan = document.createElement('span');
        dirSpan.className = 'periph-signal-dir';
        dirSpan.textContent = signal.direction === 'out' ? 'OUT' : signal.direction === 'io' ? 'I/O' : 'IN';
        tdName.appendChild(dirSpan);
        tr.appendChild(tdName);

        // Pin assignment column
        const tdPin = document.createElement('td');
        const assignedPin = reverse[signal.name];

        if (signal.fixed && signal.fixedPin) {
            // Fixed function — show pin label with assign toggle
            const pin = deviceData.pins.find(p => p.position === signal.fixedPin);
            const portName = pin && pin.port ? `R${pin.port}${pin.port_bit}` : (pin ? pin.pad_name : '?');
            const pinIsIcsp = pin && isIcspPin(pin);
            const pinIsJtag = pin && isJtagPin(pin);
            const pinBlocked = pinIsIcsp || pinIsJtag;

            const wrapper = document.createElement('label');
            wrapper.className = 'periph-fixed-assign';
            if (pinBlocked) wrapper.classList.add('blocked');

            const cb = document.createElement('input');
            cb.type = 'checkbox';
            cb.className = 'periph-fixed-cb';
            cb.dataset.signal = signal.name;
            cb.dataset.pinPos = signal.fixedPin;
            cb.dataset.direction = signal.direction;
            cb.checked = hasAssignmentFor(signal.fixedPin, signal.name);
            cb.disabled = pinBlocked;

            cb.addEventListener('change', onPeriphFixedToggle);
            wrapper.appendChild(cb);

            const labelText = document.createElement('span');
            labelText.className = 'periph-pin-fixed';
            labelText.textContent = `${portName} (pin ${signal.fixedPin})`;
            wrapper.appendChild(labelText);

            const fixedBadge = document.createElement('span');
            fixedBadge.className = 'fixed-badge';
            fixedBadge.textContent = 'fixed';
            wrapper.appendChild(fixedBadge);

            // Show other peripherals sharing this pin
            const coAssigned = getAssignmentsAt(signal.fixedPin)
                .filter(a => a.peripheral !== signal.name)
                .map(a => a.peripheral);
            if (coAssigned.length > 0) {
                const shared = document.createElement('span');
                shared.className = 'periph-shared-badge';
                shared.textContent = `shared: ${coAssigned.join(', ')}`;
                shared.title = 'Other analog functions sharing this pin';
                wrapper.appendChild(shared);
            }

            if (pinIsIcsp) {
                const icspBadge = document.createElement('span');
                icspBadge.className = 'periph-icsp-badge';
                icspBadge.textContent = 'ICSP';
                icspBadge.title = 'Reserved for ICSP/debug — not available for assignment';
                wrapper.appendChild(icspBadge);
            }
            if (pinIsJtag) {
                const jtagBadge = document.createElement('span');
                jtagBadge.className = 'periph-jtag-badge';
                jtagBadge.textContent = 'JTAG';
                jtagBadge.title = 'Reserved for JTAG while JTAGEN = ON';
                wrapper.appendChild(jtagBadge);
            }

            tdPin.appendChild(wrapper);
        } else {
            // PPS — show pin dropdown
            const select = document.createElement('select');
            select.className = 'periph-pin-select';
            select.dataset.signal = signal.name;
            select.dataset.direction = signal.direction;
            select.dataset.ppsval = signal.ppsval ?? '';

            const optNone = document.createElement('option');
            optNone.value = '';
            optNone.textContent = '\u2014 unassigned \u2014';
            select.appendChild(optNone);

            const rpPins = getAvailableRpPins(signal.name, signal.direction);

            // Group by port
            const portGroups = {};
            for (const rp of rpPins) {
                const port = rp.pin.port || '?';
                if (!portGroups[port]) portGroups[port] = [];
                portGroups[port].push(rp);
            }

            for (const [port, pins] of Object.entries(portGroups).sort()) {
                const optgroup = document.createElement('optgroup');
                optgroup.label = `Port ${port}`;
                for (const rp of pins) {
                    const opt = document.createElement('option');
                    opt.value = String(rp.pin.position);
                    let text = rp.label;
                    if (rp.usedBy) {
                        text += ` (used: ${rp.usedBy})`;
                        opt.className = 'periph-pin-used';
                    }
                    opt.textContent = text;
                    if (assignedPin === rp.pin.position) {
                        opt.selected = true;
                    }
                    optgroup.appendChild(opt);
                }
                select.appendChild(optgroup);
            }

            select.addEventListener('change', onPeriphAssignChange);
            tdPin.appendChild(select);
        }
        tr.appendChild(tdPin);

        // Signal name column
        const tdSig = document.createElement('td');
        const input = document.createElement('input');
        input.type = 'text';
        input.className = 'periph-signal-input';
        input.placeholder = 'e.g. UART1_TX';
        input.dataset.signal = signal.name;

        // If this signal is assigned to a pin, show the signal name from that pin
        if (assignedPin !== undefined && signalNames[assignedPin]) {
            input.value = signalNames[assignedPin];
        }

        input.addEventListener('focus', () => pushUndo());
        input.addEventListener('input', onPeriphSignalNameChange);
        tdSig.appendChild(input);
        tr.appendChild(tdSig);

        tbody.appendChild(tr);
    }

    table.appendChild(tbody);
    body.appendChild(table);
    card.appendChild(body);

    return card;
}
