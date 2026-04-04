/**
 * Datasheet verification flow and overlay application.
 *
 * Owns the verifier IPC flow, verification diff rendering, and the overlay
 * writeback path used to persist newly confirmed package data.
 */

/** @type {Object|null} Last verification result */
let verifyResult = null;

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
        const scoreText = isLoaded
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
    const renderProgress = (message) => {
        output.innerHTML = `
            <div class="verify-progress">
                <div class="verify-spinner"></div>
                <div class="verify-progress-text">${escapeHtml(message)}</div>
                <div class="verify-progress-time">${elapsed}s</div>
            </div>`;
    };
    const startTimer = () => {
        timerInterval = setInterval(() => {
            elapsed++;
            const timerText = output.querySelector('.verify-progress-time');
            if (timerText) {
                timerText.textContent = `${elapsed}s`;
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
        renderProgress('Looking for datasheet...');
        startTimer();

        let pdfBase64 = null;
        let pdfName = null;

        setStatus('Looking for datasheet...');
        const found = await invoke('find_datasheet', { partNumber: deviceData.part_number });

        if (found?.base64) {
            pdfBase64 = found.base64;
            pdfName = found.name;
            const source = found.source === 'downloaded' ? 'downloaded' : 'cached';
            setStatus(`Found datasheet (${source}): ${found.name}`);
        } else if (found?.text) {
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

        if (!pdfBase64) {
            setStatus('Could not obtain datasheet PDF');
            switchRightTab('code');
            if (unlisten) {
                unlisten();
            }
            stopTimer();
            return;
        }

        renderProgress(`Analyzing ${pdfName}...`);
        setStatus(`Verifying pinout from ${pdfName}...`);

        const storedKey = localStorage.getItem('pickle-api-key');
        verifyResult = await invoke('verify_pinout', {
            pdfBase64,
            partNumber: deviceData.part_number,
            package: deviceData.selected_package || null,
            apiKey: storedKey || null,
        });
        renderVerifyResult(verifyResult);
        setStatus(`Verification complete (${elapsed}s)`);
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
        output.innerHTML = '<div class="verify-error">No package data found in datasheet.</div>';
        return;
    }

    const loadedPackage = deviceData ? (deviceData.selected_package || '') : '';
    const pkgNames = matchingVerificationPackages(result);
    if (pkgNames.length === 0) {
        output.innerHTML = '<div class="verify-error">No matching packages found for this device\'s pin count.</div>';
        return;
    }

    let html = '';
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
