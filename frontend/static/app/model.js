/**
 * Browser-agnostic frontend model helpers.
 *
 * These functions stay side-effect free so the browser UI can reuse them while
 * Node-based tests verify the assignment/state logic without a DOM.
 */
(function (root, factory) {
    const api = factory(root);
    if (typeof module !== 'undefined' && module.exports) {
        module.exports = api;
    }
    root.PickleModel = api;
}(typeof globalThis !== 'undefined' ? globalThis : this, function (root) {
    const appConfig = root.PickleConfig || {
        defaults: {
            themeMode: 'dark',
            toolchain: {
                fallbackCompiler: 'xc-dsc-gcc',
                familyCompilers: {
                    pic24: 'xc16-gcc',
                    dspic33: 'xc-dsc-gcc',
                },
            },
            codegen: {
                outputBasename: 'mcu_init',
            },
        },
        theme: { modes: ['dark', 'light', 'system'] },
    };

    function defaultToolchainSettings() {
        return {
            fallback_compiler: appConfig.defaults.toolchain.fallbackCompiler,
            family_compilers: {
                pic24: appConfig.defaults.toolchain.familyCompilers.pic24,
                dspic33: appConfig.defaults.toolchain.familyCompilers.dspic33,
            },
        };
    }

    function defaultCodegenSettings() {
        return {
            output_basename: appConfig.defaults.codegen.outputBasename,
        };
    }

    function defaultAppSettings() {
        return {
            appearance: { theme: appConfig.defaults.themeMode },
            startup: { device: 'last-used', package: '' },
            toolchain: defaultToolchainSettings(),
            codegen: defaultCodegenSettings(),
            last_used: { part_number: '', package: '' },
        };
    }

    function normalizeThemeMode(mode) {
        const normalized = String(mode || '').trim().toLowerCase();
        return appConfig.theme.modes.includes(normalized)
            ? normalized
            : appConfig.defaults.themeMode;
    }

    function resolveStartupTarget(settings) {
        const startupDevice = String(settings?.startup?.device || 'last-used').trim();
        const startupPackage = String(settings?.startup?.package || '').trim();
        const lastPart = String(settings?.last_used?.part_number || '').trim().toUpperCase();
        const lastPackage = String(settings?.last_used?.package || '').trim();

        if (!startupDevice || startupDevice.toLowerCase() === 'last-used') {
            if (!lastPart) return null;
            return {
                partNumber: lastPart,
                package: lastPackage || null,
            };
        }

        return {
            partNumber: startupDevice.toUpperCase(),
            package: startupPackage || null,
        };
    }

    function detectDeviceFamily(partNumber) {
        const normalized = String(partNumber || '').trim().toUpperCase();
        if (normalized.startsWith('PIC24')) return 'pic24';
        if (normalized.startsWith('DSPIC33')) return 'dspic33';
        return 'unknown';
    }

    function resolveCompilerCommand(settings, partNumber) {
        const family = detectDeviceFamily(partNumber);
        const toolchain = settings?.toolchain || defaultToolchainSettings();
        const fallback = String(toolchain.fallback_compiler || defaultToolchainSettings().fallback_compiler).trim()
            || defaultToolchainSettings().fallback_compiler;
        const familyCompilers = toolchain.family_compilers || {};
        const resolved = family !== 'unknown'
            ? String(familyCompilers[family] || '').trim()
            : '';

        return resolved || fallback;
    }

    function normalizeOutputBasename(outputBasename) {
        const normalized = String(outputBasename || '')
            .trim()
            .toLowerCase()
            .replace(/[^a-z0-9]+/g, '_')
            .replace(/^_+|_+$/g, '');

        return normalized || defaultCodegenSettings().output_basename;
    }

    function resolveOutputBasename(settings) {
        return normalizeOutputBasename(
            settings?.codegen?.output_basename || defaultCodegenSettings().output_basename
        );
    }

    function generatedSourceFilename(settings) {
        return `${resolveOutputBasename(settings)}.c`;
    }

    function generatedHeaderFilename(settings) {
        return `${resolveOutputBasename(settings)}.h`;
    }

    function sortGeneratedFilenames(files) {
        const rank = (filename) => {
            if (filename.endsWith('.c')) return 0;
            if (filename.endsWith('.h')) return 1;
            return 2;
        };

        return Object.keys(files || {}).sort((left, right) => {
            const delta = rank(left) - rank(right);
            return delta !== 0 ? delta : left.localeCompare(right);
        });
    }

    function resolveGeneratedSourceFile(files, settings) {
        return sortGeneratedFilenames(files).find(filename => filename.endsWith('.c'))
            || generatedSourceFilename(settings);
    }

    function resolveGeneratedHeaderFile(files, settings) {
        return sortGeneratedFilenames(files).find(filename => filename.endsWith('.h'))
            || generatedHeaderFilename(settings);
    }

    function oscillatorManagedFuseFields(source, poscmd) {
        const normalizedSource = String(source || '').trim().toLowerCase();
        const normalizedPoscmd = String(poscmd || '').trim().toUpperCase();
        const baseFields = ['FNOSC', 'IESO', 'POSCMD', 'FCKSM'];

        switch (normalizedSource) {
        case 'frc':
        case 'lprc':
            return [...baseFields];
        case 'pri':
            return matchesCrystalPoscmd(normalizedPoscmd)
                ? [...baseFields, 'XTCFG']
                : [...baseFields];
        case 'frc_pll':
            return [...baseFields, 'PLLKEN'];
        case 'pri_pll':
            return matchesCrystalPoscmd(normalizedPoscmd)
                ? [...baseFields, 'PLLKEN', 'XTCFG']
                : [...baseFields, 'PLLKEN'];
        default:
            return [];
        }
    }

    function matchesCrystalPoscmd(poscmd) {
        return poscmd === 'XT' || poscmd === 'HS';
    }

    function isAnalogInput(name) {
        return /^CMP\d+[A-D]$/.test(name)
            || /^AN\d+$/.test(name)
            || /^ANA\d+$/.test(name)
            || /^OA\d+IN[+-]$/.test(name)
            || /^VREF[+-]$/.test(name);
    }

    function isAnalogOutput(name) {
        return /^OA\d+OUT$/.test(name)
            || /^DAC\d*OUT$/.test(name);
    }

    function isAnalogFunction(name) {
        return isAnalogInput(name) || isAnalogOutput(name);
    }

    function getAssignmentsAt(assignmentMap, pos) {
        const value = assignmentMap?.[pos];
        if (!value) return [];
        return Array.isArray(value) ? value : [value];
    }

    function hasAssignmentFor(assignmentMap, pos, peripheralName) {
        return getAssignmentsAt(assignmentMap, pos).some(entry => entry.peripheral === peripheralName);
    }

    function primaryAssignment(assignmentMap, pos) {
        const value = assignmentMap?.[pos];
        if (!value) return null;
        return Array.isArray(value) ? value[0] : value;
    }

    function forEachAssignedPin(assignmentMap, callback) {
        for (const pos of Object.keys(assignmentMap || {})) {
            const pinPos = parseInt(pos, 10);
            callback(pinPos, getAssignmentsAt(assignmentMap, pinPos));
        }
    }

    function flattenAssignments(assignmentMap) {
        const flattened = [];
        forEachAssignedPin(assignmentMap, (pinPos, entries) => {
            for (const entry of entries) {
                flattened.push({
                    pinPosition: pinPos,
                    rpNumber: entry.rp_number,
                    peripheral: entry.peripheral,
                    direction: entry.direction,
                    ppsval: entry.ppsval,
                    fixed: entry.fixed || false,
                });
            }
        });
        return flattened;
    }

    function buildReverseAssignments(assignmentMap) {
        const reverse = {};
        forEachAssignedPin(assignmentMap, (pinPos, entries) => {
            for (const entry of entries) {
                reverse[entry.peripheral] = pinPos;
            }
        });
        return reverse;
    }

    function normalizePositionMap(source) {
        const normalized = {};
        for (const [key, value] of Object.entries(source || {})) {
            const position = parseInt(key, 10);
            if (Number.isNaN(position)) {
                continue;
            }
            normalized[position] = value;
        }
        return normalized;
    }

    return {
        defaultAppSettings,
        defaultToolchainSettings,
        defaultCodegenSettings,
        normalizeThemeMode,
        normalizeOutputBasename,
        resolveStartupTarget,
        detectDeviceFamily,
        resolveCompilerCommand,
        resolveOutputBasename,
        generatedSourceFilename,
        generatedHeaderFilename,
        sortGeneratedFilenames,
        resolveGeneratedSourceFile,
        resolveGeneratedHeaderFile,
        oscillatorManagedFuseFields,
        isAnalogInput,
        isAnalogOutput,
        isAnalogFunction,
        getAssignmentsAt,
        hasAssignmentFor,
        primaryAssignment,
        forEachAssignedPin,
        flattenAssignments,
        buildReverseAssignments,
        normalizePositionMap,
    };
}));
