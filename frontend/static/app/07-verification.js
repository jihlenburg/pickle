/**
 * Datasheet verification workflow orchestration.
 *
 * This file owns IPC, long-running verification state, and overlay writeback.
 * Rendering and progress normalization live in dedicated verification helpers.
 */

const verificationModel = window.PickleVerificationModel || {};
const verificationRender = window.PickleVerificationRender || {};

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

function renderVerificationEmptyState() {
    return verificationRender.renderEmptyState(deviceData);
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
    let progressState = verificationModel.normalizeProgress({
        stage: 'datasheet.search',
        label: 'Looking for datasheet...',
        progress: 0.06,
    });

    const redrawProgress = () => {
        output.innerHTML = verificationRender.renderProgress(progressState, elapsed);
    };

    const renderProgress = (payload) => {
        progressState = verificationModel.normalizeProgress(payload);
        redrawProgress();
    };

    const startTimer = () => {
        timerInterval = setInterval(() => {
            elapsed += 1;
            redrawProgress();
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

        verifyResult = await invoke('verify_pinout', {
            pdfBase64: pdfBase64 || null,
            datasheetText: datasheetText || null,
            partNumber: deviceData.part_number,
            package: deviceData.selected_package || null,
            apiKey: null,
        });
        renderVerifyResult(verifyResult);
        setStatus(`Verification complete (${elapsed}s)`);
        verifyClcInBackground({
            pdfBase64,
            datasheetText,
            pdfName,
            apiKey: null,
        });
    } catch (error) {
        const message = String(error.message || error);
        output.innerHTML = `<div class="verify-error">Error: ${message.replace(/[&<>"]/g, (ch) => ({
            '&': '&amp;',
            '<': '&lt;',
            '>': '&gt;',
            '"': '&quot;',
        }[ch]))}</div>`;
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

    output.innerHTML = verificationRender.renderResultHtml({
        device: deviceData,
        result,
        siblingSource: verifySiblingSource,
    });
    verificationRender.wireResultInteractions(output, applyVerifiedOverlay);
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

        const resolvedPackageName = data.packageName || pkgName;
        setStatus(`Overlay saved for ${pkgName}. Reloading...`);
        if (button) {
            button.disabled = true;
            button.classList.add('applied');
            button.textContent = `\u2713 ${pkgName} applied`;
        }
        await loadDevice(resolvedPackageName);
        if (verifyResult) {
            renderVerifyResult(verifyResult);
        }
    } catch (error) {
        setStatus(`Error saving overlay: ${error.message || error}`);
    }
}
