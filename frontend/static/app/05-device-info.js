/**
 * Device Info tab — part decode plus package, routing, and interface summaries.
 *
 * The panel intentionally separates three sources of truth:
 * 1. selected-package pin data already loaded into `deviceData`
 * 2. routable interface signals from PPS input/output mappings
 * 3. backend `device_info` values such as memory size or ADC resolution
 *
 * That separation keeps the right panel aligned with the peripheral mapping UI
 * instead of merging unrelated counts into one misleading table.
 */

const deviceInfoModel = window.PickleModel || {};

/**
 * Decode a dsPIC33 part number into its constituent fields.
 * Structure: dsPIC33 ff mmmm xx r pp [T] [P]
 *
 * Returns null for unrecognised patterns.
 */
function decodePartNumber(part) {
    const raw = String(part || '').toUpperCase();
    const match = raw.match(
        /^D?S?PIC33(AK|CH|CK|CDVL|CDV|EDV|EV|EP)(\d+)(MPT|MP|GS|MC|GM|GP|MU)(\d)(\d{2,3})([IEH])?([A-Z]{1,3})?$/
    );
    if (!match) return null;

    const familyCodes = {
        AK: 'AK — highest performance, Safety Level L5',
        CH: 'CH — high performance, Safety Level L3–L4',
        CK: 'CK — core performance, Safety Level L4',
        CDVL: 'CDVL — integrated MOSFET driver + LIN transceiver, Safety Level L4',
        CDV: 'CDV — integrated MOSFET driver, Safety Level L4',
        EDV: 'EDV — integrated MOSFET driver (70 MHz), Safety Level L3',
        EV: 'EV — 5 V operation, Safety Level L3',
        EP: 'EP — entry performance, Safety Level L1–L2',
    };

    const typeCodes = {
        MPT: 'Motor control / Power / Touch',
        MP: 'Motor control / Power',
        GS: 'General purpose / Sensing',
        MC: 'Motor control',
        GM: 'General purpose / Motor control',
        GP: 'General purpose',
        MU: 'Motor control / USB',
    };

    const pinCodes = {
        '02': 28,
        '03': 36,
        '04': 44,
        '05': 48,
        '06': 64,
        '08': 80,
        '10': 100,
        '14': 144,
    };

    const tempCodes = {
        I: 'Industrial (−40 °C to +85 °C)',
        E: 'Extended (−40 °C to +125 °C)',
        H: 'High (−40 °C to +150 °C)',
    };

    const family = match[1];
    const flash = parseInt(match[2], 10);
    const type = match[3];
    const featureSet = parseInt(match[4], 10);
    const pinCode = match[5].length >= 2 ? match[5].slice(-2) : match[5];
    const temp = match[6] || null;
    const packageCode = match[7] || null;

    return {
        family,
        familyDesc: familyCodes[family] || family,
        flashKB: flash,
        type,
        typeDesc: typeCodes[type] || type,
        featureSet,
        pinCode,
        pinCount: pinCodes[pinCode] || null,
        tempGrade: temp,
        tempDesc: temp ? (tempCodes[temp] || temp) : null,
        packageCode,
    };
}

function formatBytes(bytes) {
    if (!bytes) return '—';
    if (bytes >= 1024) {
        const kb = bytes / 1024;
        return kb === Math.floor(kb) ? `${kb} KB` : `${kb.toFixed(1)} KB`;
    }
    return `${bytes.toLocaleString()} bytes`;
}

function formatCount(value) {
    const count = Number(value) || 0;
    return count > 0 ? count.toLocaleString() : '—';
}

function formatList(values) {
    return values && values.length ? values.join(', ') : '—';
}

function formatPreviewList(values, limit = 6) {
    if (!values || values.length === 0) {
        return '—';
    }
    if (values.length <= limit) {
        return values.join(', ');
    }
    return `${values.slice(0, limit).join(', ')} +${values.length - limit} more`;
}

function buildInfoTable(rows) {
    const visibleRows = rows.filter(([, value]) => value !== null && value !== undefined && String(value).trim() !== '');
    if (!visibleRows.length) return '';

    let html = '<table class="info-table">';
    for (const [label, value] of visibleRows) {
        html += `<tr><td class="info-label">${esc(label)}</td><td>${esc(String(value))}</td></tr>`;
    }
    html += '</table>';
    return html;
}

function buildInfoSection(title, rows, note = null) {
    const table = buildInfoTable(rows);
    if (!table && !note) return '';

    let html = '<div class="info-section">';
    html += `<h2>${esc(title)}</h2>`;
    if (table) {
        html += table;
    }
    if (note) {
        html += `<p class="info-note">${esc(note)}</p>`;
    }
    html += '</div>';
    return html;
}

function buildInfoStatGrid(stats) {
    const visibleStats = stats.filter(stat => stat && stat.value !== null && stat.value !== undefined);
    if (!visibleStats.length) return '';

    let html = '<div class="info-stat-grid">';
    for (const stat of visibleStats) {
        html += '<div class="info-stat-card">';
        html += `<div class="info-stat-label">${esc(stat.label)}</div>`;
        html += `<div class="info-stat-value">${esc(String(stat.value))}</div>`;
        if (stat.meta) {
            html += `<div class="info-stat-meta">${esc(stat.meta)}</div>`;
        }
        html += '</div>';
    }
    html += '</div>';
    return html;
}

function buildInfoCapabilityGrid(title, items, note = null) {
    const visibleItems = items.filter(item => item && item.count > 0);
    if (!visibleItems.length && !note) return '';

    let html = '<div class="info-section">';
    html += `<h2>${esc(title)}</h2>`;

    if (visibleItems.length) {
        html += '<div class="info-capability-grid">';
        for (const item of visibleItems) {
            html += '<div class="info-capability-card">';
            html += `<div class="info-capability-label">${esc(item.label)}</div>`;
            html += `<div class="info-capability-count">${esc(String(item.count))}</div>`;
            if (item.meta) {
                html += `<div class="info-capability-meta">${esc(item.meta)}</div>`;
            }
            html += '</div>';
        }
        html += '</div>';
    }

    if (note) {
        html += `<p class="info-note">${esc(note)}</p>`;
    }

    html += '</div>';
    return html;
}

function derivedInterfaceInventory(device) {
    const fallback = {
        adcChannels: 0,
        comparators: 0,
        opAmps: 0,
        dacs: 0,
        uarts: 0,
        spis: 0,
        i2c: 0,
        can: 0,
        clc: 0,
        pwm: 0,
        sccp: 0,
        sent: 0,
        qei: 0,
        timers: 0,
        labels: {
            adcChannels: [],
            comparators: [],
            opAmps: [],
            dacs: [],
            uarts: [],
            spis: [],
            i2c: [],
            can: [],
            clc: [],
            pwm: [],
            sccp: [],
            sent: [],
            qei: [],
            timers: [],
        },
    };
    return typeof deviceInfoModel.deriveDeviceInterfaceInventory === 'function'
        ? deviceInfoModel.deriveDeviceInterfaceInventory(device)
        : fallback;
}

function derivedPinStats(pins) {
    return typeof deviceInfoModel.derivePinStats === 'function'
        ? deviceInfoModel.derivePinStats(pins)
        : { remappablePins: 0, analogCapablePins: 0, utilityPins: 0, ports: [] };
}

function visibleFuseCount(fuseDefs) {
    return typeof deviceInfoModel.visibleFuseFieldCount === 'function'
        ? deviceInfoModel.visibleFuseFieldCount(fuseDefs)
        : 0;
}

function buildPartNumberRows(decoded) {
    if (!decoded) return [];

    const rows = [
        ['Family', decoded.familyDesc],
        ['Type', decoded.typeDesc],
        ['Feature set', decoded.featureSet],
        ['Part-code flash', `${decoded.flashKB} KB (nominal)`],
    ];
    if (decoded.pinCount) {
        rows.push(['Part-code pins', `${decoded.pinCount} pins`]);
    }
    if (decoded.tempDesc) {
        rows.push(['Temp grade', decoded.tempDesc]);
    }
    if (decoded.packageCode) {
        rows.push(['Orderable package code', decoded.packageCode]);
    }
    return rows;
}

function buildPackageRows(device, visiblePackagesList, packageNames, packageSource) {
    const rows = [
        ['Displayed name', displayPackageName(device.selected_package, { long: true })],
        ['Source', packageSource],
        ['Loaded pins', `${device.pin_count || device.pins?.length || 0} pins`],
        ['Available package variants', formatCount(visiblePackagesList.length)],
        ['Known package names', formatPreviewList(packageNames, 8)],
    ];

    if (displayPackageName(device.selected_package, { long: true }) !== device.selected_package) {
        rows.splice(1, 0, ['Backend key', device.selected_package]);
    }

    return rows;
}

function buildSignalFabricRows(device, pinStats, fuseFieldCount) {
    return [
        ['Resolved pins', formatCount(device.pins?.length || 0)],
        ['RP-capable package pins', formatCount(pinStats.remappablePins)],
        ['Analog-capable package pins', formatCount(pinStats.analogCapablePins)],
        ['Dedicated utility pins', formatCount(pinStats.utilityPins)],
        ['GPIO ports on package', formatList(pinStats.ports)],
        ['Remappable input signals', formatCount((device.remappable_inputs || []).length)],
        ['Remappable output signals', formatCount((device.remappable_outputs || []).length)],
        ['Input signal preview', formatPreviewList((device.remappable_inputs || []).map(signal => signal.name), 7)],
        ['Output signal preview', formatPreviewList((device.remappable_outputs || []).map(signal => signal.name), 7)],
        ['Port register blocks', formatCount(Object.keys(device.port_registers || {}).length)],
        ['Visible fuse fields', formatCount(fuseFieldCount)],
        ['CLC input groups cached', formatCount((device.clc_input_sources || []).length)],
    ];
}

function buildInterfaceItems(interfaceInventory) {
    return [
        { label: 'UART', count: interfaceInventory.uarts, meta: formatPreviewList(interfaceInventory.labels.uarts) },
        { label: 'SPI', count: interfaceInventory.spis, meta: formatPreviewList(interfaceInventory.labels.spis) },
        { label: 'I²C', count: interfaceInventory.i2c, meta: formatPreviewList(interfaceInventory.labels.i2c) },
        { label: 'SENT', count: interfaceInventory.sent, meta: formatPreviewList(interfaceInventory.labels.sent) },
        { label: 'CAN', count: interfaceInventory.can, meta: formatPreviewList(interfaceInventory.labels.can) },
        { label: 'PWM', count: interfaceInventory.pwm, meta: formatPreviewList(interfaceInventory.labels.pwm) },
        { label: 'SCCP / MCCP', count: interfaceInventory.sccp, meta: formatPreviewList(interfaceInventory.labels.sccp) },
        { label: 'QEI', count: interfaceInventory.qei, meta: formatPreviewList(interfaceInventory.labels.qei) },
        { label: 'CLC', count: interfaceInventory.clc, meta: formatPreviewList(interfaceInventory.labels.clc) },
        { label: 'Comparator', count: interfaceInventory.comparators, meta: formatPreviewList(interfaceInventory.labels.comparators) },
        { label: 'Op-Amp', count: interfaceInventory.opAmps, meta: formatPreviewList(interfaceInventory.labels.opAmps) },
        { label: 'DAC', count: interfaceInventory.dacs, meta: formatPreviewList(interfaceInventory.labels.dacs) },
        { label: 'ADC Channels', count: interfaceInventory.adcChannels, meta: formatPreviewList(interfaceInventory.labels.adcChannels) },
        { label: 'Timer Clock Inputs', count: interfaceInventory.timers, meta: formatPreviewList(interfaceInventory.labels.timers) },
    ];
}

function buildBackendRows(info) {
    const rows = [
        ['Program flash (EDC)', formatBytes(info.flash_bytes)],
        ['RAM', formatBytes(info.ram_bytes)],
    ];

    if (info.adc_max_resolution) {
        rows.push(['ADC resolution', `${info.adc_max_resolution}-bit`]);
    }
    if (info.dma_channels) {
        rows.push(['DMA channels', info.dma_channels]);
    }

    return rows;
}

/**
 * Render the Device Info tab content using the current deviceData.
 */
function renderDeviceInfo() {
    const container = document.getElementById('device-info-content');
    if (!container) return;

    const empty = document.getElementById('device-info-empty');
    if (!deviceData) {
        if (empty) empty.style.display = '';
        container.style.display = 'none';
        return;
    }

    if (empty) empty.style.display = 'none';
    container.style.display = '';

    const info = deviceData.device_info || {};
    const decoded = decodePartNumber(deviceData.part_number);
    const pins = Array.isArray(deviceData.pins) ? deviceData.pins : [];
    const selectedMeta = deviceData.packages?.[deviceData.selected_package] || null;
    const visiblePackagesList = typeof visiblePackageNames === 'function'
        ? visiblePackageNames()
        : Object.keys(deviceData.packages || {});
    const packageNames = visiblePackagesList.map(name => displayPackageName(name, {
        meta: deviceData.packages?.[name] || null,
    }));
    const pinStats = derivedPinStats(pins);
    const interfaceInventory = derivedInterfaceInventory(deviceData);
    const fuseFieldCount = visibleFuseCount(deviceData.fuse_defs);
    const packageSource = typeof packageSourceLabel === 'function'
        ? packageSourceLabel(selectedMeta)
        : (selectedMeta?.source || '—');

    let html = '';
    html += '<div class="info-hero">';
    html += `<div class="info-hero-title">${esc(deviceData.part_number)}</div>`;
    html += `<div class="info-hero-subtitle">${esc(displayPackageName(deviceData.selected_package, { long: true }))} • ${esc(packageSource)} • ${esc(String(deviceData.pin_count || pins.length || '—'))} pins loaded</div>`;
    html += '</div>';

    html += buildInfoStatGrid([
        {
            label: 'Packages',
            value: formatCount(visiblePackagesList.length),
            meta: packageNames.length ? formatPreviewList(packageNames, 4) : null,
        },
        {
            label: 'RP Pins',
            value: formatCount(pinStats.remappablePins),
            meta: 'Pins with PPS remap numbers',
        },
        {
            label: 'Ports',
            value: formatCount(pinStats.ports.length),
            meta: formatList(pinStats.ports),
        },
        {
            label: 'PPS Inputs',
            value: formatCount((deviceData.remappable_inputs || []).length),
            meta: formatPreviewList((deviceData.remappable_inputs || []).map(signal => signal.name), 4),
        },
        {
            label: 'PPS Outputs',
            value: formatCount((deviceData.remappable_outputs || []).length),
            meta: formatPreviewList((deviceData.remappable_outputs || []).map(signal => signal.name), 4),
        },
        {
            label: 'Visible Fuses',
            value: formatCount(fuseFieldCount),
            meta: 'Editable config fields',
        },
    ]);

    html += buildInfoSection('Part Number', buildPartNumberRows(decoded));
    html += buildInfoSection(
        'Package & Pinout',
        buildPackageRows(deviceData, visiblePackagesList, packageNames, packageSource)
    );
    html += buildInfoSection('Signal Fabric', buildSignalFabricRows(deviceData, pinStats, fuseFieldCount));
    html += buildInfoCapabilityGrid(
        'Pin-Exposed Interfaces',
        buildInterfaceItems(interfaceInventory),
        'Derived from the loaded package pins plus PPS remappable input/output signals. This reflects what the current package exposes to the outside world, which is why these counts can be more useful than the sparse backend inventory on newer families.'
    );
    html += buildInfoSection(
        'Memory & Backend Parse',
        buildBackendRows(info),
        'Memory size and ADC resolution come from the backend device parser. Interface coverage above is intentionally signal-driven instead of relying on incomplete backend counts.'
    );

    container.innerHTML = html;
}

/**
 * Escape HTML entities.
 */
function esc(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}
