/**
 * Saved configuration import/export flows.
 *
 * Serializes editor state into JSON and restores it after device reloads,
 * including reservation stashes that the runtime keeps outside the main map.
 */

// =============================================================================
// Save / Load Configuration
// =============================================================================

/** Save the current configuration (assignments, signals, osc, fuses) via a native file dialog. */
async function saveConfig() {
    if (!deviceData) return;
    const config = {
        part_number: deviceData.part_number,
        package: deviceData.selected_package,
        assignments: assignments,
        signal_names: signalNames,
        // Preserve fuse-driven temporary state so toggling routing/debug fuses
        // after a reload can still restore the user's previous pin choices.
        reserved_assignments: {
            jtag: jtagReservedAssignments,
            i2c: i2cRoutedAssignments,
        },
        oscillator: getOscConfig(),
        fuses: getFuseConfig(),
        clc: getClcConfig(),
    };
    try {
        const result = await invoke('save_text_file_dialog', {
            request: {
                title: 'Save Pin Configuration',
                suggestedName: `${deviceData.part_number}_${deviceData.selected_package}.json`,
                contents: JSON.stringify(config, null, 2),
                filters: [{ name: 'JSON', extensions: ['json'] }],
            },
        });
        if (result) {
            setStatus(`Saved config to ${result.path}`);
        }
    } catch (e) {
        setStatus('Error saving config: ' + (e.message || e));
    }
}

/** Open a previously saved configuration JSON file via a native file picker. */
async function openConfigDialog() {
    try {
        const result = await invoke('open_text_file_dialog', {
            request: {
                title: 'Open Pin Configuration',
                filters: [{ name: 'JSON', extensions: ['json'] }],
            },
        });
        if (!result) return;
        await loadConfigText(result.contents, result.path);
    } catch (e) {
        setStatus('Error loading config: ' + (e.message || e));
    }
}

/**
 * Restore a configuration JSON string and reload the selected device.
 * @param {string} text - JSON configuration text
 * @param {string} [sourcePath] - Optional source path shown in the status text
 */
async function loadConfigText(text, sourcePath) {
    try {
        const config = JSON.parse(text);

        if (!config.part_number) {
            setStatus('Invalid config file: missing part_number');
            return;
        }

        $('part-input').value = config.part_number;

        // Seed state before rendering so the freshly loaded device paints the saved config
        // during the first render instead of flashing an empty assignment table.
        assignments = model.normalizePositionMap(config.assignments);
        signalNames = model.normalizePositionMap(config.signal_names);
        jtagReservedAssignments = model.normalizePositionMap(config.reserved_assignments?.jtag);
        i2cRoutedAssignments = model.normalizePositionMap(config.reserved_assignments?.i2c);

        applyClcConfig(config.clc);

        await loadDevice(config.package || null, { preserveState: true });
        applyOscillatorConfig(config.oscillator);
        applyFuseSelections(config.fuses?.selections);
        const sourceName = sourcePath ? ` from ${sourcePath.split(/[\\/]/).pop()}` : '';
        setStatus(`Loaded config${sourceName}: ${config.part_number} — ${config.package || 'default'}`);
    } catch (e) {
        setStatus('Error loading config: ' + (e.message || e));
    }
}
