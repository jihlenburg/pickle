// ---------------------------------------------------------------------------
// Device Info tab — part number decoder + peripheral/memory overview
// ---------------------------------------------------------------------------

/**
 * Decode a dsPIC33 part number into its constituent fields.
 * Structure: dsPIC33 ff mmmm xx r pp [T] [P]
 *
 * Returns null for unrecognised patterns.
 */
function decodePartNumber(part) {
    // Normalise: uppercase, strip leading "dspic" or "pic" prefix for matching
    const raw = part.toUpperCase();
    const m = raw.match(
        /^D?S?PIC33(AK|CH|CK|CDVL|CDV|EDV|EV|EP)(\d+)(MPT|MP|GS|MC|GM|GP|MU)(\d)(\d{2,3})([IEH])?([A-Z]{1,3})?$/
    );
    if (!m) return null;

    const familyCodes = {
        AK:   'AK — highest performance, Safety Level L5',
        CH:   'CH — high performance, Safety Level L3–L4',
        CK:   'CK — core performance, Safety Level L4',
        CDVL: 'CDVL — integrated MOSFET driver + LIN transceiver, Safety Level L4',
        CDV:  'CDV — integrated MOSFET driver, Safety Level L4',
        EDV:  'EDV — integrated MOSFET driver (70 MHz), Safety Level L3',
        EV:   'EV — 5 V operation, Safety Level L3',
        EP:   'EP — entry performance, Safety Level L1–L2',
    };

    const typeCodes = {
        MPT: 'Motor control / Power / Touch',
        MP:  'Motor control / Power',
        GS:  'General purpose / Sensing',
        MC:  'Motor control',
        GM:  'General purpose / Motor control',
        GP:  'General purpose',
        MU:  'Motor control / USB',
    };

    const pinCodes = {
        '02': 28, '03': 36, '04': 44, '05': 48,
        '06': 64, '08': 80, '10': 100, '14': 144,
    };

    const tempCodes = {
        I: 'Industrial (−40 °C to +85 °C)',
        E: 'Extended (−40 °C to +125 °C)',
        H: 'High (−40 °C to +150 °C)',
    };

    const ff   = m[1];
    const flash = parseInt(m[2], 10);
    const type  = m[3];
    const feat  = parseInt(m[4], 10);
    const ppRaw = m[5];
    const temp  = m[6] || null;
    const pkg   = m[7] || null;

    // pp may be 2 or 3 digits (e.g. "06" or "106" where "1" is a prefix)
    // Try last 2 digits first as that's the pin-count code
    const ppCode = ppRaw.length >= 2 ? ppRaw.slice(-2) : ppRaw;
    const pins = pinCodes[ppCode] || null;

    return {
        family:      ff,
        familyDesc:  familyCodes[ff] || ff,
        flashKB:     flash,
        type:        type,
        typeDesc:    typeCodes[type] || type,
        featureSet:  feat,
        pinCode:     ppCode,
        pinCount:    pins,
        tempGrade:   temp,
        tempDesc:    temp ? (tempCodes[temp] || temp) : null,
        packageCode: pkg,
    };
}

/**
 * Format a byte count as a human-friendly string (KB or bytes).
 */
function formatBytes(bytes) {
    if (bytes === 0) return '—';
    if (bytes >= 1024) {
        const kb = bytes / 1024;
        // Show integer KB when exact, otherwise one decimal place
        return kb === Math.floor(kb) ? `${kb} KB` : `${kb.toFixed(1)} KB`;
    }
    return `${bytes.toLocaleString()} bytes`;
}

/**
 * Render the Device Info tab content using the current deviceData.
 */
function renderDeviceInfo() {
    const container = document.getElementById('device-info-content');
    if (!container) return;

    const empty = document.getElementById('device-info-empty');

    if (!deviceData || !deviceData.device_info) {
        if (empty) empty.style.display = '';
        container.style.display = 'none';
        return;
    }

    if (empty) empty.style.display = 'none';
    container.style.display = '';

    const info = deviceData.device_info;
    const decoded = decodePartNumber(deviceData.part_number);

    let html = '';

    // Part number decode section
    if (decoded) {
        html += '<div class="info-section">';
        html += '<h2>Part Number</h2>';
        html += '<table class="info-table">';
        html += `<tr><td class="info-label">Family</td><td>${esc(decoded.familyDesc)}</td></tr>`;
        html += `<tr><td class="info-label">Flash</td><td>${decoded.flashKB} KB (nominal)</td></tr>`;
        html += `<tr><td class="info-label">Type</td><td>${esc(decoded.typeDesc)}</td></tr>`;
        html += `<tr><td class="info-label">Feature set</td><td>${decoded.featureSet}</td></tr>`;
        if (decoded.pinCount) {
            html += `<tr><td class="info-label">Pin count</td><td>${decoded.pinCount} pins</td></tr>`;
        }
        if (decoded.tempDesc) {
            html += `<tr><td class="info-label">Temp grade</td><td>${esc(decoded.tempDesc)}</td></tr>`;
        }
        if (decoded.packageCode) {
            html += `<tr><td class="info-label">Package</td><td>${esc(decoded.packageCode)}</td></tr>`;
        }
        html += '</table></div>';
    }

    // Memory section
    html += '<div class="info-section">';
    html += '<h2>Memory</h2>';
    html += '<table class="info-table">';
    html += `<tr><td class="info-label">Flash</td><td>${formatBytes(info.flash_bytes)}</td></tr>`;
    html += `<tr><td class="info-label">RAM</td><td>${formatBytes(info.ram_bytes)}</td></tr>`;
    html += '</table></div>';

    // Peripherals section — only show non-zero counts
    const peripherals = [
        ['ADC channels',    info.adc_channels],
        ['ADC resolution',  info.adc_max_resolution ? `${info.adc_max_resolution}-bit` : null],
        ['Comparators',     info.comparators],
        ['DAC channels',    info.dac_channels],
        ['Op-amps',         info.op_amps],
        ['PWM generators',  info.pwm_generators],
        ['SCCP/MCCP',       info.sccp_mccp],
        ['Timers',          info.timers],
        ['UARTs',           info.uarts],
        ['SPI',             info.spis],
        ['I²C',             info.i2c],
        ['CAN',             info.can],
        ['CLC',             info.clc],
        ['SENT',            info.sent],
        ['DMA channels',    info.dma_channels],
        ['QEI',             info.qei],
    ];

    const visible = peripherals.filter(([, v]) => v && v !== 0);

    if (visible.length > 0) {
        html += '<div class="info-section">';
        html += '<h2>Peripherals</h2>';
        html += '<table class="info-table">';
        for (const [label, value] of visible) {
            html += `<tr><td class="info-label">${label}</td><td>${value}</td></tr>`;
        }
        html += '</table></div>';
    }

    container.innerHTML = html;
}

/**
 * Escape HTML entities.
 */
function esc(s) {
    const div = document.createElement('div');
    div.textContent = s;
    return div.innerHTML;
}
