/**
 * Code generation and export workflows.
 *
 * Bridges the current editor state to backend code generation, code-tab
 * switching, clipboard copy, and plain-text export helpers.
 */

// =============================================================================
// Code Generation
// =============================================================================

/**
 * Request code generation from the backend and display the result.
 * Sends all current assignments, oscillator config, and fuse config.
 * Handles both multi-file and single-file responses using the configured
 * output basename instead of hard-coded filenames.
 */
async function generateCode() {
    if (!deviceData) return;

    const assignmentList = flattenAssignments();

    if (assignmentList.length === 0) {
        generatedFiles = {};
        activeTab = model.generatedSourceFilename(appSettings);
        setTextContent('code-output', '// No pin assignments configured.');
        hideElement('code-tabs');
        return;
    }

    try {
        const payload = {
            partNumber: deviceData.part_number,
            package: deviceData.selected_package,
            assignments: assignmentList,
            signalNames: signalNames,
            digitalPins: [],
            oscillator: getOscConfig(),
            fuses: getFuseConfig(),
            clc: getClcConfig(),
        };
        const data = await invoke('generate_code', { request: payload });
        if (data.files) {
            generatedFiles = data.files;
            renderCodeTabs();
        } else {
            const sourceFile = model.generatedSourceFilename(appSettings);
            generatedFiles = { [sourceFile]: data.code };
            renderCodeTabs();
        }
    } catch (e) {
        setTextContent('code-output', '// Error generating code: ' + (e.message || e));
    }
}

function orderedGeneratedFiles() {
    return model.sortGeneratedFilenames(generatedFiles);
}

function renderCodeTabs() {
    const container = $('code-tabs');
    const filenames = orderedGeneratedFiles();

    if (!container) return;
    container.innerHTML = '';

    if (filenames.length === 0) {
        hideElement(container);
        setTextContent('code-output', '');
        return;
    }

    activeTab = filenames.includes(activeTab)
        ? activeTab
        : model.resolveGeneratedSourceFile(generatedFiles, appSettings);
    if (!filenames.includes(activeTab)) {
        activeTab = filenames[0];
    }

    for (const filename of filenames) {
        const button = document.createElement('button');
        button.className = 'code-tab';
        button.dataset.file = filename;
        button.textContent = filename;
        container.appendChild(button);
    }

    if (filenames.length > 1) {
        showElement(container, 'flex');
    } else {
        hideElement(container);
    }

    showTab(activeTab);
}

/**
 * Switch the visible code output tab.
 * @param {string} tab - Filename to display (for example `mcu_init.c`)
 */
function showTab(tab) {
    const filenames = orderedGeneratedFiles();
    if (filenames.length === 0) {
        activeTab = model.generatedSourceFilename(appSettings);
        setTextContent('code-output', '');
        return;
    }

    activeTab = filenames.includes(tab) ? tab : filenames[0];
    setTextContent('code-output', generatedFiles[activeTab] || '');
    for (const btn of document.querySelectorAll('.code-tab')) {
        btn.classList.toggle('active', btn.dataset.file === activeTab);
    }
}

/** Copy the currently visible code output to the clipboard. */
async function copyCode() {
    const code = $('code-output').textContent;
    try {
        await navigator.clipboard.writeText(code);
        setStatus('Code copied to clipboard');
        flashButtonLabel('copy-btn', 'Copied!', 'Copy');
    } catch (e) {
        setStatus('Error: clipboard access failed');
    }
}

/** Export all generated files using a native folder picker. */
async function exportCode() {
    const files = Object.entries(generatedFiles);
    if (files.length === 0) return;

    try {
        const result = await invoke('export_generated_files_dialog', {
            request: {
                title: 'Export Generated C Files',
                files: generatedFiles,
            },
        });
        if (!result) return;

        setStatus(`Exported ${result.writtenFiles.length} files to ${result.directory}`);
        flashButtonLabel('export-btn', 'Exported!', 'Export Files');
    } catch (e) {
        setStatus('Error exporting files: ' + (e.message || e));
    }
}

/**
 * Export a formatted pin list as a plain text file for documentation.
 * Produces a clean table with pin number, name, assignment, and signal name.
 */
async function exportPinList() {
    if (!deviceData) return;

    const lines = [];
    const part = deviceData.part_number;
    const pkg = deviceData.selected_package;
    const date = new Date().toISOString().slice(0, 10);

    lines.push(`${part} — ${pkg}`);
    lines.push(`Pin Assignment List (${date})`);
    lines.push('');

    // Build rows and measure column widths before rendering the grid.
    const rows = [];
    for (const pin of deviceData.pins) {
        const num = String(pin.position);
        const name = pin.port ? `R${pin.port}${pin.port_bit}` : pin.pad_name || pin.functions[0] || '—';
        const pinAssigns = getAssignmentsAt(pin.position);
        const assign = pinAssigns.length > 0
            ? pinAssigns.map(a => a.peripheral).join(', ')
            : (pin.is_power ? pin.functions[0] : '—');
        const sig = signalNames[pin.position] || '';
        rows.push([num, name, assign, sig]);
    }

    const headers = ['Pin', 'Name', 'Function', 'Signal'];
    const widths = headers.map((h, i) =>
        Math.max(h.length, ...rows.map(r => r[i].length))
    );

    const grid = document.getElementById('pinlist-grid')?.checked ?? true;

    const fmtRow = grid
        ? (cols) => cols.map((c, i) => ` ${c.padEnd(widths[i])} `).join('│')
        : (cols) => cols.map((c, i) => c.padEnd(widths[i])).join('  ');

    lines.push(fmtRow(headers));
    if (grid) {
        lines.push(widths.map(w => '─'.repeat(w + 2)).join('┼'));
    } else {
        lines.push(widths.map(w => '─'.repeat(w)).join('──'));
    }
    for (const row of rows) {
        lines.push(fmtRow(row));
    }

    lines.push('');
    lines.push('Generated by pickle');

    const text = lines.join('\n');
    try {
        const result = await invoke('save_text_file_dialog', {
            request: {
                title: 'Save Pin Assignment List',
                suggestedName: `${part}_pinlist.txt`,
                contents: text,
                filters: [{ name: 'Text', extensions: ['txt'] }],
            },
        });
        if (result) {
            setStatus(`Saved pin list to ${result.path}`);
        }
    } catch (e) {
        setStatus('Error saving pin list: ' + (e.message || e));
    }
}
