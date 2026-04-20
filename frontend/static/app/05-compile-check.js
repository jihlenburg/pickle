/**
 * Compiler-backed validation workflows.
 *
 * Resolves the correct Microchip compiler per device family and keeps the UI
 * copy aligned with backend toolchain detection results.
 */

// =============================================================================
// Compiler Check
// =============================================================================

/** @type {boolean} Whether the resolved compiler for the active device is available. */
let compilerAvailable = false;
/** @type {string} Compiler executable resolved for the active device family. */
let compilerCommand = model.resolveCompilerCommand(appSettings, null);
/** @type {string} Device family key used when resolving the active compiler. */
let compilerFamily = 'unknown';

function currentCompilerPartNumber() {
    if (deviceData?.part_number) {
        return deviceData.part_number;
    }
    const part = $('part-input')?.value?.trim().toUpperCase();
    return part || null;
}

function compilerFamilyLabel(family) {
    return appConfig.ui.compiler.familyLabels[family] || appConfig.ui.compiler.familyLabels.unknown;
}

/** Check if the configured compiler for the active device family is available. */
async function checkCompiler(partNumber = currentCompilerPartNumber()) {
    compilerFamily = model.detectDeviceFamily(partNumber);
    compilerCommand = model.resolveCompilerCommand(appSettings, partNumber);
    $('check-btn').textContent = appConfig.ui.compiler.buttonFallbackLabel;
    hideElement('check-btn');

    if (!partNumber) {
        return;
    }

    try {
        const data = await invoke('compiler_info', { partNumber });
        compilerAvailable = data.available;
        compilerCommand = String(data.command || compilerCommand).trim() || compilerCommand;
        compilerFamily = data.device_family || compilerFamily;

        if (compilerAvailable) {
            $('check-btn').textContent = appConfig.format(appConfig.ui.compiler.buttonLabel, {
                compiler: compilerCommand,
            });
            showElement('check-btn');
            $('check-btn').title = data.version || `${compilerCommand} (${compilerFamilyLabel(compilerFamily)})`;
        }
    } catch (e) {
        compilerAvailable = false;
    }
}

/** Send the currently displayed code to the backend for a family-aware compiler check. */
async function compileCheck() {
    if (!deviceData) return;
    const code = $('code-output').textContent;
    if (!code || code.startsWith('//') || code.startsWith('Load a device')) return;

    const resultBox = $('compile-result');
    resultBox.className = 'compile-result';
    resultBox.textContent = appConfig.ui.compiler.checkingStatus;

    try {
        const sourceFile = model.resolveGeneratedSourceFile(generatedFiles, appSettings);
        const headerFile = model.resolveGeneratedHeaderFile(generatedFiles, appSettings);
        const data = await invoke('compile_check', {
            request: {
                code: generatedFiles[sourceFile] || code,
                header: generatedFiles[headerFile] || '',
                partNumber: deviceData.part_number,
            },
        });
        compilerCommand = String(data.command || compilerCommand).trim() || compilerCommand;
        compilerFamily = data.device_family || compilerFamily;

        if (data.success && !data.warnings) {
            resultBox.className = 'compile-result success';
            resultBox.textContent = appConfig.format(appConfig.ui.compiler.successMessage, {
                compiler: compilerCommand,
            });
        } else if (data.success && data.warnings) {
            resultBox.className = 'compile-result warning';
            resultBox.textContent = appConfig.format(appConfig.ui.compiler.warningMessage, {
                compiler: compilerCommand,
                details: data.warnings,
            });
        } else {
            resultBox.className = 'compile-result error';
            const errorMessage = appConfig.format(appConfig.ui.compiler.failureMessage, {
                compiler: compilerCommand,
                details: data.errors,
            });
            resultBox.textContent = errorMessage;
            window.PickleUI.toast(errorMessage, {
                tone: 'error',
                title: 'Compile error',
                action: { label: 'Show', onClick: () => scrollToCompileResult() },
            });
        }
    } catch (e) {
        resultBox.className = 'compile-result error';
        const errorMessage = 'Error: ' + (e.message || e);
        resultBox.textContent = errorMessage;
        window.PickleUI.toast(errorMessage, {
            tone: 'error',
            title: 'Compile error',
            action: { label: 'Show', onClick: () => scrollToCompileResult() },
        });
    }
}

/** Scroll the compile-result panel into view and highlight it briefly. */
function scrollToCompileResult() {
    const box = $('compile-result');
    if (!box || typeof box.scrollIntoView !== 'function') return;
    box.scrollIntoView({ behavior: 'smooth', block: 'center' });
}
