/**
 * Datasheet verification flow and overlay application.
 *
 * Owns the verifier IPC flow, verification diff rendering, and the overlay
 * writeback path used to persist newly confirmed package data.
 */

/** @type {Object|null} Last verification result */
let verifyResult = null;
/** @type {string|null} Sibling part slug when datasheet came from a related device */
let verifySiblingSource = null;
let clcVerifyJob = null;
let clcVerifyPartNumber = null;

/** Check if API key is configured. */
async function checkApiKey() {
    try {
        const data = await invoke('api_key_status');
        const button = $('verify-btn');
        if (button) {
            button.title = data.configured
                ? `API key configured (${data.hint})`
                : 'No API key — configure in .env';
        }
        return data.configured;
    } catch {
        return false;
    }
}

/** Normalize pad name for comparison (strip trailing _N suffixes). */
function normalizePad(name) {
    return (name || '').toUpperCase().replace(/_\d+$/, '');
}

/** Escape HTML special characters. */
function escapeHtml(str) {
    const div = document.createElement('div');
    div.textContent = str;
    return div.innerHTML;
}

function verificationScoreClass(matchScore) {
    if (matchScore >= 0.95) return 'score-good';
    if (matchScore >= 0.8) return 'score-warn';
    return 'score-bad';
}

function currentPinMap() {
    const pins = {};
    if (deviceData?.pins) {
        deviceData.pins.forEach((pin) => {
            pins[pin.position] = pin;
        });
    }
    return pins;
}

function matchingVerificationPackages(result) {
    const devicePinCount = deviceData ? deviceData.pin_count : 0;
    return Object.keys(result.packages).filter((name) => {
        const pkg = result.packages[name];
        return !devicePinCount || pkg.pin_count === devicePinCount;
    });
}

function renderVerificationCorrections(pkg) {
    if (!pkg.corrections?.length) {
        return '';
    }

    let html = '<div class="verify-corrections">';
    html += `<h4>Corrections (${pkg.corrections.length})</h4>`;
    pkg.corrections.forEach((correction) => {
        const typeLabel = {
            wrong_pad: 'Wrong Pad',
            missing_functions: 'Missing Functions',
            extra_functions: 'Extra Functions',
            missing_pin: 'Missing Pin',
            extra_pin: 'Extra Pin',
        }[correction.correction_type] || correction.correction_type;

        html += '<div class="verify-corr-item">';
        html += `<span class="verify-corr-type">${typeLabel}</span> `;
        html += `Pin ${correction.pin_position}: `;
        if (correction.current_pad) {
            html += `<span class="verify-old">${escapeHtml(correction.current_pad)}</span>`;
        }
        if (correction.current_pad && correction.datasheet_pad) {
            html += ' \u2192 ';
        }
        if (correction.datasheet_pad) {
            html += `<span class="verify-new">${escapeHtml(correction.datasheet_pad)}</span>`;
        }
        if (correction.note) {
            html += ` <span class="verify-corr-note">${escapeHtml(correction.note)}</span>`;
        }
        html += '</div>';
    });
    html += '</div>';
    return html;
}

function buildVerificationRows(pkg, currentPins, isLoaded) {
    const sortedPositions = Object.keys(pkg.pins).map(Number).sort((a, b) => a - b);
    let matchCount = 0;
    let totalCompared = 0;
    let rows = '';

    for (const position of sortedPositions) {
        const datasheetPad = pkg.pins[position];

        if (!isLoaded) {
            rows += '<tr class="verify-ok">';
            rows += `<td>${position}</td>`;
            rows += `<td colspan="2">${escapeHtml(datasheetPad)}</td>`;
            rows += '<td></td>';
            rows += '</tr>';
            continue;
        }

        const currentPin = currentPins[position];
        const currentPad = currentPin ? (currentPin.pad_name || currentPin.pad) : '\u2014';
        const match = currentPin && normalizePad(datasheetPad) === normalizePad(currentPad);
        if (currentPin) totalCompared++;
        if (match) matchCount++;

        const statusClass = match ? 'verify-ok' : currentPin ? 'verify-diff' : 'verify-new';
        const statusText = match ? '\u2713' : currentPin ? '\u2260' : 'NEW';

        rows += `<tr class="${statusClass}">`;
        rows += `<td>${position}</td>`;
        rows += `<td>${escapeHtml(datasheetPad)}</td>`;
        rows += `<td>${escapeHtml(currentPad)}</td>`;
        rows += `<td class="status-icon">${statusText}</td>`;
        rows += '</tr>';
    }

    return { rows, matchCount, totalCompared };
}

function verificationSummaryHtml(isLoaded, totalCompared, matchCount) {
    if (!isLoaded) {
        return '<div class="verify-summary verify-new-pkg">New package \u2014 not in current EDC data. Apply as overlay to use it.</div>';
    }
    if (totalCompared > 0 && matchCount === totalCompared) {
        return `<div class="verify-match">All ${totalCompared} pins match the loaded EDC data.</div>`;
    }
    if (totalCompared > 0) {
        const diffCount = totalCompared - matchCount;
        return `<div class="verify-summary">${matchCount}/${totalCompared} pins match \u2014 ${diffCount} difference${diffCount > 1 ? 's' : ''} found.</div>`;
    }
    return '';
}

function renderVerificationTabs(pkgNames, loadedPackage, result) {
    let html = '<div class="verify-pkg-tabs">';
    pkgNames.forEach((name) => {
        const pkg = result.packages[name];
        const isLoaded = name.toUpperCase() === loadedPackage.toUpperCase();
        const correctionCount = (pkg.corrections || []).length;
        const scoreText = pkg.match_score != null
            ? ` <span class="verify-score ${verificationScoreClass(pkg.match_score)}">${Math.round(pkg.match_score * 100)}%</span>`
            : '';
        const badge = correctionCount > 0 ? ` <span class="verify-corr-badge">${correctionCount}</span>` : '';
        const active = name === (pkgNames.find((pkgName) => pkgName.toUpperCase() === loadedPackage.toUpperCase()) || pkgNames[0])
            ? ' active'
            : '';

        html += `<button class="verify-pkg-tab${active}" data-pkg="${name}">`;
        html += `${escapeHtml(name)} (${pkg.pin_count}p)${scoreText}${badge}</button>`;
    });
    html += '</div>';
    return html;
}

function renderVerificationDetails(pkgNames, loadedPackage, currentPins, result) {
    const defaultTab = pkgNames.find((name) => name.toUpperCase() === loadedPackage.toUpperCase()) || pkgNames[0];
    let html = '';

    pkgNames.forEach((name) => {
        const pkg = result.packages[name];
        const isLoaded = name.toUpperCase() === loadedPackage.toUpperCase();
        const hidden = name === defaultTab ? '' : ' hidden';
        const { rows, matchCount, totalCompared } = buildVerificationRows(pkg, currentPins, isLoaded);
        const alreadyApplied = !!(
            deviceData?.packages &&
            Object.keys(deviceData.packages).some((packageName) => packageName.toUpperCase() === name.toUpperCase())
        );

        html += `<div class="verify-pkg-detail${hidden}" data-pkg="${name}">`;
        if (isLoaded) {
            html += renderVerificationCorrections(pkg);
        }
        html += verificationSummaryHtml(isLoaded, totalCompared, matchCount);
        html += '<div class="verify-table-wrap"><table class="verify-table"><thead><tr>';
        if (isLoaded) {
            html += '<th>Pin</th><th>Datasheet</th><th>EDC Parser</th><th class="status-icon"></th>';
        } else {
            html += '<th>Pin</th><th colspan="2">Datasheet</th><th></th>';
        }
        html += `</tr></thead><tbody>${rows}</tbody></table></div>`;

        if (alreadyApplied) {
            html += `<button class="verify-apply-btn applied" data-pkg="${name}" disabled>\u2713 ${escapeHtml(name)} applied</button>`;
        } else {
            html += `<button class="verify-apply-btn" data-pkg="${name}">Apply "${escapeHtml(name)}" as Overlay</button>`;
        }

        html += '</div>';
    });

    return html;
}

function wireVerificationResultInteractions(output) {
    output.querySelectorAll('.verify-pkg-tab').forEach((tab) => {
        tab.addEventListener('click', () => {
            output.querySelectorAll('.verify-pkg-tab').forEach((button) => {
                button.classList.remove('active');
            });
            tab.classList.add('active');
            output.querySelectorAll('.verify-pkg-detail').forEach((detail) => {
                detail.classList.add('hidden');
            });
            output.querySelector(`.verify-pkg-detail[data-pkg="${tab.dataset.pkg}"]`)?.classList.remove('hidden');
        });
    });

    output.querySelectorAll('.verify-apply-btn').forEach((button) => {
        button.addEventListener('click', () => applyVerifiedOverlay(button.dataset.pkg));
    });
}

const VERIFY_PROGRESS_STEPS = [
    { id: 'datasheet', label: 'Get datasheet' },
    { id: 'device', label: 'Load device' },
    { id: 'reduce', label: 'Trim pages' },
    { id: 'upload', label: 'Upload' },
    { id: 'analyze', label: 'AI analysis' },
    { id: 'process', label: 'Process result' },
];

const VERIFY_STAGE_TO_STEP = {
    'datasheet.search': 'datasheet',
    'datasheet.resolve': 'datasheet',
    'datasheet.download': 'datasheet',
    'datasheet.decode': 'datasheet',
    'datasheet.ready': 'datasheet',
    'device.load': 'device',
    'provider.select': 'device',
    'datasheet.reduce': 'reduce',
    'provider.render': 'reduce',
    'provider.upload': 'upload',
    'provider.analyze': 'analyze',
    'result.cached': 'process',
    'result.process': 'process',
    'result.done': 'process',
};

const VERIFY_STAGE_PROGRESS = {
    'datasheet.search': 0.06,
    'datasheet.resolve': 0.1,
    'datasheet.download': 0.16,
    'datasheet.decode': 0.24,
    'datasheet.ready': 0.34,
    'device.load': 0.3,
    'provider.select': 0.36,
    'datasheet.reduce': 0.44,
    'provider.render': 0.62,
    'provider.upload': 0.58,
    'provider.analyze': 0.76,
    'result.cached': 0.95,
    'result.process': 0.94,
    'result.done': 1.0,
};

function normalizeVerifyProgress(payload) {
    if (payload && typeof payload === 'object' && !Array.isArray(payload)) {
        const stage = String(payload.stage || 'legacy');
        return {
            stage,
            label: String(payload.label || payload.message || 'Working...'),
            detail: payload.detail ? String(payload.detail) : '',
            progress: typeof payload.progress === 'number'
                ? payload.progress
                : (VERIFY_STAGE_PROGRESS[stage] || 0.1),
            indeterminate: !!payload.indeterminate,
            provider: payload.provider ? String(payload.provider) : '',
        };
    }

    return {
        stage: 'legacy',
        label: String(payload || 'Working...'),
        detail: '',
        progress: 0.1,
        indeterminate: false,
        provider: '',
    };
}

function verifyProgressHint(progress) {
    if (progress.stage === 'datasheet.reduce') {
        return 'pickle uploads only the pinout pages in the main verification pass. CLC pages can be checked separately in the background.';
    }
    if (progress.stage === 'provider.render') {
        return 'The PDF path needed a retry, so pickle is rendering only the selected datasheet pages as 300 DPI PNGs.';
    }
    if (progress.stage === 'provider.analyze') {
        return 'pickle is waiting for the provider to extract the package tables before it can show the pinout comparison.';
    }
    if (progress.stage === 'result.cached') {
        return 'pickle found a cached verification result for this exact datasheet, so it skipped the provider call.';
    }
    if (progress.stage === 'result.done') {
        return 'The extracted package data is ready to review and apply as an overlay if needed.';
    }
    return 'Verification time varies by provider and datasheet size. Large datasheets can take up to 3 minutes or more.';
}

function renderVerifyProgress(progress, elapsed) {
    const normalized = normalizeVerifyProgress(progress);
    const currentStep = VERIFY_STAGE_TO_STEP[normalized.stage] || 'datasheet';
    const currentStepIndex = VERIFY_PROGRESS_STEPS.findIndex((step) => step.id === currentStep);
    const percent = Math.max(6, Math.min(100, Math.round(normalized.progress * 100)));

    const stepsHtml = VERIFY_PROGRESS_STEPS.map((step, index) => {
        let state = 'pending';
        if (index < currentStepIndex) {
            state = 'done';
        } else if (index === currentStepIndex) {
            state = 'active';
        }

        return `
            <div class="verify-progress-step is-${state}">
                <div class="verify-progress-step-dot">${state === 'done' ? '\u2713' : index + 1}</div>
                <div class="verify-progress-step-label">${escapeHtml(step.label)}</div>
            </div>`;
    }).join('');

    const providerBadge = normalized.provider
        ? `<div class="verify-progress-provider">${escapeHtml(normalized.provider)}</div>`
        : '';
    const detailHtml = normalized.detail
        ? `<div class="verify-progress-detail">${escapeHtml(normalized.detail)}</div>`
        : '';
    const hint = verifyProgressHint(normalized);
    const hintHtml = hint
        ? `<div class="verify-progress-hint">${escapeHtml(hint)}</div>`
        : '';

    return `
        <div class="verify-progress-card">
            <div class="verify-progress-head">
                <div class="verify-progress-copy">
                    <div class="verify-progress-eyebrow">Datasheet Verification</div>
                    <div class="verify-progress-text">${escapeHtml(normalized.label)}</div>
                    ${detailHtml}
                </div>
                <div class="verify-progress-side">
                    ${providerBadge}
                    <div class="verify-progress-time">${elapsed}s</div>
                </div>
            </div>
            <div class="verify-progress-bar-track">
                <div class="verify-progress-bar-fill${normalized.indeterminate ? ' is-indeterminate' : ''}" style="width:${percent}%"></div>
            </div>
            ${hintHtml}
            <div class="verify-progress-steps">${stepsHtml}</div>
        </div>`;
}

function renderVerificationTimingNote() {
    const clcNote = deviceData?.has_clc
        ? 'CLC input sources can be looked up in a second background pass if they are still missing after pinout verification.'
        : 'This device has no CLC peripheral, so no background CLC lookup will be run.';
    return `
        <div class="verify-expectation">
            pickle trims the datasheet to the pinout pages before upload. ${clcNote}
        </div>`;
}

function renderSiblingDatasheetNotice() {
    if (!verifySiblingSource || !deviceData) {
        return '';
    }
    return `
        <div class="verify-sibling-notice">
            <strong>Note:</strong> No dedicated datasheet was found for
            <strong>${escapeHtml(deviceData.part_number)}</strong>.
            Verification is using the sibling family datasheet from
            <strong>${escapeHtml(verifySiblingSource)}</strong>
            (same pin-number suffix). Pin assignments should match,
            but double-check against the official datasheet when it
            becomes available.
        </div>`;
}

function renderSyntheticPackageNotice() {
    if (!deviceData || !isSyntheticPackage(deviceData.selected_package)) {
        return '';
    }

    return `
        <div class="verify-synthetic-notice">
            <strong>${escapeHtml(displayPackageName(deviceData.selected_package, { long: true }))}</strong> is a fallback package from the EDC, not a real package name. Verify against the datasheet to import the actual package name and pin table.
        </div>`;
}

function renderVerificationEmptyState() {
    return `
        ${renderSyntheticPackageNotice()}
        <div class="verify-empty">Load a device and click <strong>Verify Pinout</strong> to cross-check pin assignments against the datasheet.</div>`;
}

async function verifyClcInBackground({ pdfBase64, datasheetText, pdfName, apiKey }) {
    if (!deviceData?.part_number || !deviceData?.has_clc || deviceData.clc_input_sources) {
        return;
    }
    if (clcVerifyJob && clcVerifyPartNumber === deviceData.part_number) {
        return;
    }

    const partNumber = deviceData.part_number;
    const packageName = deviceData.selected_package || null;
    clcVerifyPartNumber = partNumber;

    clcVerifyJob = (async () => {
        try {
            setStatus(`Pinout verified. Looking up CLC sources in background from ${pdfName}...`);
            const clcResult = await invoke('verify_clc', {
                pdfBase64: pdfBase64 || null,
                datasheetText: datasheetText || null,
                partNumber,
                package: packageName,
                apiKey: apiKey || null,
            });

            if (!deviceData || deviceData.part_number !== partNumber) {
                return;
            }

            const notes = Array.isArray(clcResult?.notes) ? clcResult.notes : [];
            if (verifyResult && notes.length) {
                const existing = new Set(verifyResult.notes || []);
                notes.forEach((note) => existing.add(note));
                verifyResult.notes = Array.from(existing);
            }

            if (Array.isArray(clcResult?.clc_input_sources) && clcResult.clc_input_sources.length === 4) {
                deviceData.clc_input_sources = clcResult.clc_input_sources;
                if (verifyResult) {
                    verifyResult.clc_input_sources = clcResult.clc_input_sources;
                }
                if (typeof renderClcDesigner === 'function') {
                    renderClcDesigner();
                }
                if (verifyResult) {
                    renderVerifyResult(verifyResult);
                }
                setStatus(`Pinout verified. CLC sources imported from ${pdfName}.`);
            } else {
                if (verifyResult) {
                    renderVerifyResult(verifyResult);
                }
                setStatus('Pinout verified. Background CLC lookup found no CLC register data.');
            }
        } catch (error) {
            if (deviceData?.part_number === partNumber) {
                setStatus(`Pinout verified. Background CLC lookup failed: ${error.message || error}`);
            }
        } finally {
            clcVerifyJob = null;
            clcVerifyPartNumber = null;
        }
    })();
}

/** Trigger pinout verification — auto-finds datasheet, falls back to file dialog. */
async function verifyPinout() {
    if (!deviceData) {
        setStatus('Load a device first');
        return;
    }

    await checkApiKey();

    const output = $('verify-output');
    if (!output) {
        return;
    }

    switchRightTab('verify');

    let unlisten = null;
    let elapsed = 0;
    let timerInterval = null;
    let progressState = normalizeVerifyProgress({
        stage: 'datasheet.search',
        label: 'Looking for datasheet...',
        progress: 0.06,
    });
    const renderProgress = (payload) => {
        progressState = normalizeVerifyProgress(payload);
        output.innerHTML = renderVerifyProgress(progressState, elapsed);
    };
    const startTimer = () => {
        timerInterval = setInterval(() => {
            elapsed++;
            if (progressState) {
                output.innerHTML = renderVerifyProgress(progressState, elapsed);
            }
        }, 1000);
    };
    const stopTimer = () => {
        if (timerInterval) {
            clearInterval(timerInterval);
            timerInterval = null;
        }
    };

    try {
        if (window.__TAURI__?.event?.listen) {
            unlisten = await window.__TAURI__.event.listen('verify-progress', (event) => {
                renderProgress(event.payload);
            });
        }

        elapsed = 0;
        renderProgress({
            stage: 'datasheet.search',
            label: 'Looking for datasheet...',
            progress: 0.06,
        });
        startTimer();

        let pdfBase64 = null;
        let datasheetText = null;
        let pdfName = null;

        setStatus('Looking for datasheet...');
        const found = await invoke('find_datasheet', { partNumber: deviceData.part_number });

        if (found?.base64) {
            pdfBase64 = found.base64;
            pdfName = found.name;
            verifySiblingSource = found.sibling_source || null;
            const source = found.source === 'downloaded' ? 'downloaded' : 'cached';
            if (verifySiblingSource) {
                setStatus(`Using sibling family datasheet (${source}): ${found.datasheet_title || found.name}`);
            } else {
                setStatus(`Found datasheet (${source}): ${found.name}`);
            }
        } else if (found?.text) {
            datasheetText = found.text;
            pdfName = found.name;
            setStatus(`Using text extraction: ${found.name}`);
        } else {
            stopTimer();
            setStatus('No datasheet found — please select one');
            const file = await invoke('open_binary_file_dialog', {
                request: {
                    title: 'Select Datasheet PDF',
                    filters: [{ name: 'PDF', extensions: ['pdf'] }],
                },
            });
            if (!file) {
                switchRightTab('code');
                if (unlisten) {
                    unlisten();
                }
                return;
            }
            pdfBase64 = file.base64;
            pdfName = file.name;
            elapsed = 0;
            startTimer();
        }

        if (!pdfBase64 && !datasheetText) {
            setStatus('Could not obtain datasheet input');
            switchRightTab('code');
            if (unlisten) {
                unlisten();
            }
            stopTimer();
            return;
        }
        if (!pdfBase64 && datasheetText) {
            setStatus('Verification requires a datasheet PDF; text-only fallback is disabled');
            switchRightTab('code');
            if (unlisten) {
                unlisten();
            }
            stopTimer();
            return;
        }

        renderProgress({
            stage: 'datasheet.ready',
            label: `Prepared ${pdfName} for verification`,
            detail: 'pickle will now trim the datasheet and send only the relevant pages to the provider.',
            progress: 0.34,
        });
        setStatus(`Verifying pinout from ${pdfName}...`);

        const storedKey = localStorage.getItem('pickle-api-key');
        verifyResult = await invoke('verify_pinout', {
            pdfBase64: pdfBase64 || null,
            datasheetText: datasheetText || null,
            partNumber: deviceData.part_number,
            package: deviceData.selected_package || null,
            apiKey: storedKey || null,
        });
        renderVerifyResult(verifyResult);
        setStatus(`Verification complete (${elapsed}s)`);
        verifyClcInBackground({
            pdfBase64,
            datasheetText,
            pdfName,
            apiKey: storedKey,
        });
    } catch (error) {
        output.innerHTML = `<div class="verify-error">Error: ${escapeHtml(String(error.message || error))}</div>`;
        setStatus('Verification error');
    } finally {
        if (unlisten) {
            unlisten();
        }
        stopTimer();
    }
}

/** Render the verification diff result. */
function renderVerifyResult(result) {
    const output = $('verify-output');
    if (!output) {
        return;
    }

    if (!result?.packages || Object.keys(result.packages).length === 0) {
        output.innerHTML = `${renderSyntheticPackageNotice()}<div class="verify-error">No package data found in datasheet.</div>`;
        return;
    }

    const loadedPackage = deviceData ? (deviceData.selected_package || '') : '';
    const pkgNames = matchingVerificationPackages(result);
    if (pkgNames.length === 0) {
        output.innerHTML = '<div class="verify-error">No matching packages found for this device\'s pin count.</div>';
        return;
    }

    let html = '';
    html += renderSiblingDatasheetNotice();
    html += renderSyntheticPackageNotice();
    html += renderVerificationTimingNote();
    if (result.notes?.length) {
        html += '<div class="verify-notes">';
        result.notes.forEach((note) => {
            html += `<div class="verify-note">${escapeHtml(note)}</div>`;
        });
        html += '</div>';
    }

    html += renderVerificationTabs(pkgNames, loadedPackage, result);
    html += renderVerificationDetails(pkgNames, loadedPackage, currentPinMap(), result);
    output.innerHTML = html;
    wireVerificationResultInteractions(output);
}

/** Apply a verified package overlay to the device. */
async function applyVerifiedOverlay(pkgName) {
    if (!verifyResult?.packages?.[pkgName]) {
        return;
    }

    const pkg = verifyResult.packages[pkgName];
    const request = {
        partNumber: verifyResult.part_number,
        packages: {
            [pkgName]: {
                pin_count: pkg.pin_count,
                pins: pkg.pins,
                pin_functions: pkg.pin_functions || {},
            },
        },
    };

    const button = document.querySelector(`.verify-apply-btn[data-pkg="${pkgName}"]`);
    try {
        const data = await invoke('apply_overlay', { request });
        if (!data.success) {
            setStatus('Failed to save overlay');
            return;
        }

        setStatus(`Overlay saved for ${pkgName}. Reloading...`);
        if (button) {
            button.disabled = true;
            button.classList.add('applied');
            button.textContent = `\u2713 ${pkgName} applied`;
        }
        await loadDevice(pkgName);
    } catch (error) {
        setStatus(`Error saving overlay: ${error.message || error}`);
    }
}
