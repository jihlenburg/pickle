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

    function normalizeVerificationProvider(provider) {
        const normalized = String(provider || '').trim().toLowerCase();
        return ['auto', 'openai', 'anthropic'].includes(normalized)
            ? normalized
            : 'auto';
    }

    function defaultAppSettings() {
        return {
            appearance: { theme: appConfig.defaults.themeMode },
            startup: { device: 'last-used', package: '' },
            toolchain: defaultToolchainSettings(),
            codegen: defaultCodegenSettings(),
            verification: { provider: 'auto' },
            onboarding: { welcome_intro_seen: false },
            last_used: { part_number: '', package: '' },
        };
    }

    function normalizeThemeMode(mode) {
        const normalized = String(mode || '').trim().toLowerCase();
        return appConfig.theme.modes.includes(normalized)
            ? normalized
            : appConfig.defaults.themeMode;
    }

    function normalizePackageLabelText(name) {
        return String(name || '')
            .normalize('NFKC')
            .replace(/[‐‑‒–—−]/g, '-')
            .replace(/\s+/g, ' ')
            .trim();
    }

    function stripPackageDeviceQualifier(name) {
        const rawName = normalizePackageLabelText(name);
        const qualifierMatch = rawName.match(/^(.*?)\s+\(([^)]*(?:DSPIC|PIC24)[^)]*)\)$/i);
        if (!qualifierMatch) {
            return rawName;
        }
        return qualifierMatch[1].trim() || rawName;
    }

    function normalizeFriendlyPackageName(name) {
        const rawName = stripPackageDeviceQualifier(name);
        if (!rawName) {
            return '—';
        }

        // Device packs sometimes expose internal package codes such as
        // `STX04 (48-pin uQFN)`. Normalize those to the public-facing package
        // family so the UI does not leak backend identifiers.
        const codedPackageMatch = rawName.match(/^[A-Z]{2,}\d+\s+\((\d+)-pin\s+(u?QFN|VQFN|TQFP)\)$/i);
        if (!codedPackageMatch) {
            return rawName;
        }

        const pinCount = codedPackageMatch[1];
        const family = codedPackageMatch[2].toUpperCase() === 'TQFP' ? 'TQFP' : 'VQFN';
        return `${pinCount}-PIN ${family}`;
    }

    function editablePackageDisplayName(name, meta = null) {
        const explicitDisplayName = normalizePackageLabelText(meta?.display_name || '');
        if (explicitDisplayName) {
            return normalizeFriendlyPackageName(explicitDisplayName);
        }
        return normalizeFriendlyPackageName(name);
    }

    function packageIdentity(name, meta = null) {
        const backendKey = String(name || '').trim();
        const pinCount = Number(meta?.pin_count);
        const source = String(meta?.source || '').trim().toLowerCase() === 'overlay'
            ? 'overlay'
            : 'edc';
        const synthetic = backendKey.toLowerCase() === 'default' && source === 'edc';
        const defaultDisplayName = synthetic
            ? (backendKey || '—')
            : normalizeFriendlyPackageName(backendKey);
        const explicitDisplayName = normalizePackageLabelText(meta?.display_name || '');
        const displayName = explicitDisplayName
            ? normalizeFriendlyPackageName(explicitDisplayName)
            : defaultDisplayName;
        const canonicalDisplayName = normalizePackageLabelText(displayName).toUpperCase();

        return {
            backendKey,
            pinCount: Number.isFinite(pinCount) ? pinCount : 0,
            source,
            synthetic,
            defaultDisplayName,
            displayName,
            hasDisplayOverride: Boolean(explicitDisplayName),
            identityKey: `${Number.isFinite(pinCount) ? pinCount : 0}|${canonicalDisplayName}`,
        };
    }

    function packageIdentityKey(name, meta = null) {
        return packageIdentity(name, meta).identityKey;
    }

    function packagePreferenceScore(name, meta = null) {
        let score = 0;
        const identity = packageIdentity(name, meta);
        if (identity.source === 'overlay') {
            score += 10;
        }
        if (identity.hasDisplayOverride) {
            score += 2;
        }
        if (identity.displayName === identity.backendKey) {
            score += 1;
        }
        return score;
    }

    function visiblePackageEntries(packages) {
        const entries = Object.entries(packages || {});
        const realPinCounts = new Set(
            entries
                .filter(([name, meta]) => !(String(name).trim().toLowerCase() === 'default' && meta?.source === 'edc'))
                .map(([, meta]) => meta?.pin_count)
                .filter((pinCount) => typeof pinCount === 'number')
        );

        const visible = [];
        const winners = new Map();
        for (const [name, meta] of entries) {
            const identity = packageIdentity(name, meta);
            if (identity.synthetic) {
                if (!realPinCounts.has(meta?.pin_count)) {
                    visible.push(identity);
                }
                continue;
            }

            const key = identity.identityKey;
            const currentWinner = winners.get(key);
            if (!currentWinner) {
                winners.set(key, identity);
                continue;
            }

            const currentScore = packagePreferenceScore(currentWinner.backendKey, packages?.[currentWinner.backendKey]);
            const nextScore = packagePreferenceScore(name, meta);
            if (
                nextScore > currentScore
                || (nextScore === currentScore && String(name).localeCompare(currentWinner.backendKey) < 0)
            ) {
                winners.set(key, identity);
            }
        }

        visible.push(...Array.from(winners.values()));
        return visible;
    }

    function visiblePackageNames(packages) {
        return visiblePackageEntries(packages).map(entry => entry.backendKey);
    }

    function preferredVisiblePackage(packages, selectedName) {
        const selected = String(selectedName || '').trim();
        if (!selected || !packages?.[selected]) {
            return selected ? packageIdentity(selected, packages?.[selected] || null) : null;
        }

        const selectedIdentity = packageIdentity(selected, packages[selected]);
        if (selectedIdentity.synthetic) {
            return selectedIdentity;
        }

        let preferred = selectedIdentity;
        for (const [name, meta] of Object.entries(packages || {})) {
            if (name === selected) {
                continue;
            }
            const identity = packageIdentity(name, meta);
            if (identity.synthetic || identity.identityKey !== selectedIdentity.identityKey) {
                continue;
            }

            const preferredScore = packagePreferenceScore(preferred.backendKey, packages?.[preferred.backendKey]);
            const candidateScore = packagePreferenceScore(name, meta);
            if (
                candidateScore > preferredScore
                || (candidateScore === preferredScore && String(name).localeCompare(preferred.backendKey) < 0)
            ) {
                preferred = identity;
            }
        }

        return preferred;
    }

    function preferredVisiblePackageName(packages, selectedName) {
        return preferredVisiblePackage(packages, selectedName)?.backendKey || String(selectedName || '').trim();
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

    function extractDeviceBranch(partNumber) {
        const normalized = String(partNumber || '').trim().toUpperCase();
        const match = normalized.match(/([A-Z]{2,})\d{3}[A-Z]*$/);
        return match ? match[1] : '';
    }

    function detectDeviceFamily(partNumber) {
        const normalized = String(partNumber || '').trim().toUpperCase();
        if (normalized.startsWith('PIC24')) return 'pic24';
        if (normalized.startsWith('DSPIC33')) return 'dspic33';
        return 'unknown';
    }

    function resolveDeviceProfile(partNumber) {
        const normalized = String(partNumber || '').trim().toUpperCase();
        const family = detectDeviceFamily(normalized);
        const isAk = normalized.startsWith('DSPIC33AK');
        const branch = extractDeviceBranch(normalized);

        return {
            partNumber: normalized,
            family,
            architecture: isAk ? 'dspic33ak' : family,
            branch,
            series: isAk ? `dspic33ak-${(branch || 'generic').toLowerCase()}` : family,
            instructionClock: isAk ? 'fosc' : 'fosc_div_2',
            managesOscillatorFuses: !isAk,
        };
    }

    function isDsPic33AkPart(partNumber) {
        return resolveDeviceProfile(partNumber).architecture === 'dspic33ak';
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

    function oscillatorManagedFuseFields(source, poscmd, partNumber) {
        if (!resolveDeviceProfile(partNumber).managesOscillatorFuses) {
            return [];
        }

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

    function oscillatorTargetHint(targetFoscMhz, partNumber) {
        const value = Number(targetFoscMhz);
        if (!(value > 0)) {
            return '';
        }

        const profile = resolveDeviceProfile(partNumber);
        if (profile.instructionClock === 'fosc') {
            return `Fcy = ${value.toFixed(3)} MHz (dsPIC33AK: Fcy = Fosc)`;
        }

        return `Fcy = ${(value / 2).toFixed(3)} MHz`;
    }

    function isAnalogInput(name) {
        return /^CMP\d+[A-D]$/.test(name)
            || /^AN\d+$/.test(name)
            || /^ANA\d+$/.test(name)
            || /^AD\d+AN[A-Z]*\d+$/.test(name)
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

    function mergeFuseField(existingField, incomingField) {
        if (!existingField) {
            return incomingField;
        }
        if (!incomingField) {
            return existingField;
        }
        return {
            ...existingField,
            ...incomingField,
            desc: existingField.desc || incomingField.desc || '',
            values: (existingField.values && existingField.values.length)
                ? existingField.values
                : (incomingField.values || []),
            hidden: Boolean(existingField.hidden && incomingField.hidden),
        };
    }

    function normalizeFuseDefinitions(fuseDefs) {
        const registers = [];
        const registerIndex = new Map();

        for (const rawRegister of fuseDefs || []) {
            const registerName = String(rawRegister?.cname || '').trim();
            if (!registerName) {
                continue;
            }

            let normalizedRegister = registerIndex.get(registerName);
            if (!normalizedRegister) {
                normalizedRegister = {
                    ...rawRegister,
                    cname: registerName,
                    fields: [],
                };
                registerIndex.set(registerName, normalizedRegister);
                registers.push(normalizedRegister);
            } else if (!normalizedRegister.desc && rawRegister?.desc) {
                normalizedRegister.desc = rawRegister.desc;
            }

            const fieldIndex = new Map(
                (normalizedRegister.fields || []).map(field => [String(field?.cname || '').trim(), field])
            );

            for (const rawField of rawRegister.fields || []) {
                const fieldName = String(rawField?.cname || '').trim();
                if (!fieldName) {
                    continue;
                }
                const existingField = fieldIndex.get(fieldName);
                const mergedField = mergeFuseField(existingField, {
                    ...rawField,
                    cname: fieldName,
                });
                if (!existingField) {
                    normalizedRegister.fields.push(mergedField);
                    fieldIndex.set(fieldName, mergedField);
                } else {
                    const existingIndex = normalizedRegister.fields.indexOf(existingField);
                    normalizedRegister.fields.splice(existingIndex, 1, mergedField);
                    fieldIndex.set(fieldName, mergedField);
                }
            }
        }

        return registers;
    }

    function fuseGroupId(registerName, fieldNames) {
        const normalizedName = String(registerName || '').toUpperCase();
        const upperFields = fieldNames.map(name => String(name || '').toUpperCase());

        if (
            normalizedName.startsWith('FOSC')
            || upperFields.some(name => ['FNOSC', 'POSCMD', 'FCKSM', 'IESO', 'PLLKEN', 'XTCFG'].includes(name))
        ) {
            return 'clock';
        }
        if (
            normalizedName === 'FICD'
            || upperFields.some(name => ['ICS', 'JTAGEN', 'BKBUG', 'NOBTSWP'].includes(name))
        ) {
            return 'debug';
        }
        if (
            normalizedName.startsWith('FWDT')
            || upperFields.some(name => name.includes('WDT'))
        ) {
            return 'watchdog';
        }
        if (
            normalizedName.startsWith('FPR')
            || upperFields.some(name => ['CP', 'WRP', 'RWP', 'PWP'].some(token => name.includes(token)))
        ) {
            return 'protection';
        }
        if (
            ['FBOOT', 'FCP', 'FSECDBG', 'FPED'].includes(normalizedName)
            || upperFields.some(name => ['BOOT', 'SEC', 'CP', 'PED'].some(token => name.includes(token)))
        ) {
            return 'bootSecurity';
        }
        if (
            normalizedName === 'FIRT'
            || upperFields.some(name => ['BIST', 'IRT'].some(token => name.includes(token)))
        ) {
            return 'integrity';
        }
        if (normalizedName === 'FDEVOPT') {
            return 'device';
        }
        return 'other';
    }

    function groupedFuseDefinitions(fuseDefs) {
        const normalizedRegisters = normalizeFuseDefinitions(fuseDefs);
        const configuredLabels = appConfig.ui?.fuses?.groups || {};
        const groupOrder = [
            'clock',
            'debug',
            'watchdog',
            'bootSecurity',
            'protection',
            'integrity',
            'device',
            'other',
        ];
        const groups = groupOrder.map((id) => ({
            id,
            label: configuredLabels[id] || id,
            registers: [],
        }));
        const groupMap = new Map(groups.map(group => [group.id, group]));

        for (const register of normalizedRegisters) {
            const visibleFields = (register.fields || []).filter(field => !field.hidden);
            if (!visibleFields.length) {
                continue;
            }
            const id = fuseGroupId(register.cname, visibleFields.map(field => field.cname));
            groupMap.get(id)?.registers.push({
                ...register,
                fields: visibleFields,
            });
        }

        return groups.filter(group => group.registers.length > 0);
    }

    function visibleFuseFieldCount(fuseDefs) {
        return normalizeFuseDefinitions(fuseDefs).reduce((count, register) => {
            const visibleFields = (register.fields || []).filter(field => !field.hidden);
            return count + visibleFields.length;
        }, 0);
    }

    function derivePinStats(pins) {
        const uniquePorts = new Set();
        let remappablePins = 0;
        let analogCapablePins = 0;
        let utilityPins = 0;

        for (const pin of pins || []) {
            if (typeof pin?.rp_number === 'number') {
                remappablePins += 1;
            }
            if (pin?.port) {
                uniquePorts.add(pin.port);
            }
            if ((pin?.functions || []).some(fn => isAnalogFunction(fn) || /^AN[A-Z]?\d+$/i.test(fn))) {
                analogCapablePins += 1;
            }
            if (pin?.is_power) {
                utilityPins += 1;
            }
        }

        return {
            remappablePins,
            analogCapablePins,
            utilityPins,
            ports: Array.from(uniquePorts).sort(),
        };
    }

    function createInterfaceInventory() {
        return {
            adcChannels: new Set(),
            comparators: new Set(),
            opAmps: new Set(),
            dacs: new Set(),
            uarts: new Set(),
            spis: new Set(),
            i2c: new Set(),
            can: new Set(),
            clc: new Set(),
            pwm: new Set(),
            sccp: new Set(),
            sent: new Set(),
            qei: new Set(),
            timers: new Set(),
        };
    }

    function naturalLabelCompare(left, right) {
        return left.localeCompare(right, undefined, {
            numeric: true,
            sensitivity: 'base',
        });
    }

    function sortInterfaceLabels(values) {
        return Array.from(values || []).sort(naturalLabelCompare);
    }

    function addInterfaceLabel(target, label) {
        if (label) {
            target.add(label);
        }
    }

    function extractPeripheralInstance(rawName) {
        const name = String(rawName || '').trim().toUpperCase();
        if (!name) {
            return null;
        }

        let match;
        if ((match = name.match(/^U(\d+)/))) return { type: 'UART', instance: match[1], id: `UART${match[1]}` };
        if ((match = name.match(/^(?:SDI|SDO|SCK|SS)(\d+)/))) return { type: 'SPI', instance: match[1], id: `SPI${match[1]}` };
        if ((match = name.match(/^A?(?:SCL|SDA)(\d+)$/))) return { type: 'I2C', instance: match[1], id: `I2C${match[1]}` };
        if ((match = name.match(/^C(\d+)(?:TX|RX)$/))) return { type: 'CAN', instance: match[1], id: `CAN${match[1]}` };
        if ((match = name.match(/^SENT(\d+)/))) return { type: 'SENT', instance: match[1], id: `SENT${match[1]}` };
        if ((match = name.match(/^PWM(\d+)[HL]$/))) return { type: 'PWM', instance: match[1], id: `PWM${match[1]}` };
        if ((match = name.match(/^QEI(?:A|B|HOME|INDX)(\d+)$/))) return { type: 'QEI', instance: match[1], id: `QEI${match[1]}` };
        if ((match = name.match(/^QEI(\d+)CMP$/))) return { type: 'QEI', instance: match[1], id: `QEI${match[1]}` };
        if ((match = name.match(/^T(\d+)CK$/))) return { type: 'Timer', instance: match[1], id: `Timer${match[1]}` };
        if ((match = name.match(/^TCKI(\d+)$/))) return { type: 'Timer', instance: match[1], id: `Timer${match[1]}` };
        if ((match = name.match(/^ICM(\d+)$/))) return { type: 'Input Capture', instance: match[1], id: `ICM${match[1]}` };
        if ((match = name.match(/^OCM(\d+)/))) return { type: 'CCP', instance: match[1], id: `CCP${match[1]}` };
        if ((match = name.match(/^CMP(\d+)[A-D]?$/))) return { type: 'Comparator', instance: match[1], id: `CMP${match[1]}` };
        if ((match = name.match(/^CLC(\d+)/))) return { type: 'CLC', instance: match[1], id: `CLC${match[1]}` };
        if ((match = name.match(/^INT(\d+)$/))) return { type: 'Interrupt', instance: match[1], id: `INT${match[1]}` };
        if ((match = name.match(/^PCI(\d+)$/))) return { type: 'PWM Fault', instance: match[1], id: `PCI${match[1]}` };
        if ((match = name.match(/^ADCTRG(\d+)$/))) return { type: 'ADC Trigger', instance: match[1], id: `ADCTRG${match[1]}` };
        if ((match = name.match(/^PTGTRG(\d+)$/))) return { type: 'PTG', instance: match[1], id: `PTG${match[1]}` };
        if ((match = name.match(/^OA(\d+)/))) return { type: 'Op-Amp', instance: match[1], id: `OA${match[1]}` };
        if ((match = name.match(/^AD(\d+)AN[A-Z]*\d+$/))) {
            return { type: 'ADC', instance: match[1], id: `ADC${match[1]}` };
        }
        if ((match = name.match(/^ANA(\d+)$/))) {
            return {
                type: 'ADC',
                instance: String(parseInt(match[1], 10) + 1),
                id: `ADC${match[1]}`,
                label: `ADC${match[1]} (dedicated)`,
            };
        }
        if (/^AN\d+$/.test(name)) {
            return {
                type: 'ADC',
                instance: '0',
                id: 'ADC',
                label: 'ADC (shared)',
            };
        }
        if (/^DAC\d*OUT$/.test(name)) return { type: 'DAC', instance: '0', id: 'DAC' };
        if ((match = name.match(/^IBIAS(\d+)$/))) return { type: 'Bias', instance: '0', id: 'Bias Current' };
        if (/^PWME[A-Z]$/.test(name)) return { type: 'PWM Event', instance: '0', id: 'PWM Events' };
        if (/^PWMTRG/.test(name)) return { type: 'PWM Trigger', instance: '0', id: 'PWM Triggers' };
        if (/^OCF[A-Z]$/.test(name)) return { type: 'CCP Fault', instance: '0', id: 'CCP Faults' };
        if (/^CLCIN/.test(name)) return { type: 'CLC Input', instance: '0', id: 'CLC Inputs' };
        if (/^REF[IO]/.test(name)) return { type: 'Reference Clock', instance: '0', id: 'Ref Clock' };
        if (/^RPV\d/.test(name)) return { type: 'Virtual Pin', instance: '0', id: 'Virtual Pins' };
        return null;
    }

    function addInterfaceSignal(inventory, rawName) {
        const name = String(rawName || '').trim().toUpperCase();
        if (!name) {
            return;
        }

        let match;
        if ((match = name.match(/^AN([A-Z]?\d+)$/))) addInterfaceLabel(inventory.adcChannels, `AN${match[1]}`);
        if ((match = name.match(/^AD\d+AN([A-Z]?\d+)$/))) addInterfaceLabel(inventory.adcChannels, `AN${match[1]}`);
        if ((match = name.match(/^CMP(\d+)/))) addInterfaceLabel(inventory.comparators, `CMP${match[1]}`);
        if ((match = name.match(/^OA(\d+)/))) addInterfaceLabel(inventory.opAmps, `OA${match[1]}`);
        if (name === 'DACOUT') {
            addInterfaceLabel(inventory.dacs, 'DAC1');
        } else if ((match = name.match(/^DAC(\d+)OUT$/))) {
            addInterfaceLabel(inventory.dacs, `DAC${match[1]}`);
        }
        if ((match = name.match(/^U(\d+)(?:RX|TX|CTS|RTS|DTR|DSR)$/))) addInterfaceLabel(inventory.uarts, `UART${match[1]}`);
        if ((match = name.match(/^(?:SCK|SDI|SDO|SS)(\d+)$/))) addInterfaceLabel(inventory.spis, `SPI${match[1]}`);
        if ((match = name.match(/^(?:A?SCL|A?SDA)(\d+)$/))) addInterfaceLabel(inventory.i2c, `I2C${match[1]}`);
        if ((match = name.match(/^C(\d+)(?:TX|RX)$/))) addInterfaceLabel(inventory.can, `CAN${match[1]}`);
        if ((match = name.match(/^CLC(\d+)OUT$/))) addInterfaceLabel(inventory.clc, `CLC${match[1]}`);
        if ((match = name.match(/^CLC(\d+)$/))) addInterfaceLabel(inventory.clc, `CLC${match[1]}`);
        if ((match = name.match(/^PWM(\d+)[HL]?$/))) addInterfaceLabel(inventory.pwm, `PWM${match[1]}`);
        if ((match = name.match(/^(?:CCP|ICM|OCM)(\d+)/))) addInterfaceLabel(inventory.sccp, `SCCP${match[1]}`);
        if ((match = name.match(/^SENT(\d+)(?:TX|IN)?$/))) addInterfaceLabel(inventory.sent, `SENT${match[1]}`);
        if ((match = name.match(/^QEI(?:A|B|HOME|INDX)(\d+)$/))) addInterfaceLabel(inventory.qei, `QEI${match[1]}`);
        if ((match = name.match(/^QEI(\d+)CMP$/))) addInterfaceLabel(inventory.qei, `QEI${match[1]}`);
        if ((match = name.match(/^T(\d+)CKI?$/))) addInterfaceLabel(inventory.timers, `T${match[1]}`);
        if ((match = name.match(/^TCKI(\d+)$/))) addInterfaceLabel(inventory.timers, `T${match[1]}`);
    }

    function deriveDeviceInterfaceInventory(deviceData) {
        const inventory = createInterfaceInventory();

        for (const signal of deviceData?.remappable_inputs || []) {
            addInterfaceSignal(inventory, signal?.name);
        }
        for (const signal of deviceData?.remappable_outputs || []) {
            addInterfaceSignal(inventory, signal?.name);
        }
        for (const pin of deviceData?.pins || []) {
            for (const fn of pin?.functions || []) {
                addInterfaceSignal(inventory, fn);
            }
        }

        return {
            adcChannels: inventory.adcChannels.size,
            comparators: inventory.comparators.size,
            opAmps: inventory.opAmps.size,
            dacs: inventory.dacs.size,
            uarts: inventory.uarts.size,
            spis: inventory.spis.size,
            i2c: inventory.i2c.size,
            can: inventory.can.size,
            clc: inventory.clc.size,
            pwm: inventory.pwm.size,
            sccp: inventory.sccp.size,
            sent: inventory.sent.size,
            qei: inventory.qei.size,
            timers: inventory.timers.size,
            labels: {
                adcChannels: sortInterfaceLabels(inventory.adcChannels),
                comparators: sortInterfaceLabels(inventory.comparators),
                opAmps: sortInterfaceLabels(inventory.opAmps),
                dacs: sortInterfaceLabels(inventory.dacs),
                uarts: sortInterfaceLabels(inventory.uarts),
                spis: sortInterfaceLabels(inventory.spis),
                i2c: sortInterfaceLabels(inventory.i2c),
                can: sortInterfaceLabels(inventory.can),
                clc: sortInterfaceLabels(inventory.clc),
                pwm: sortInterfaceLabels(inventory.pwm),
                sccp: sortInterfaceLabels(inventory.sccp),
                sent: sortInterfaceLabels(inventory.sent),
                qei: sortInterfaceLabels(inventory.qei),
                timers: sortInterfaceLabels(inventory.timers),
            },
        };
    }

    return {
        defaultAppSettings,
        defaultToolchainSettings,
        defaultCodegenSettings,
        normalizeVerificationProvider,
        normalizeThemeMode,
        normalizeFriendlyPackageName,
        packageIdentity,
        packageIdentityKey,
        visiblePackageEntries,
        visiblePackageNames,
        preferredVisiblePackage,
        preferredVisiblePackageName,
        normalizeOutputBasename,
        resolveStartupTarget,
        resolveDeviceProfile,
        detectDeviceFamily,
        resolveCompilerCommand,
        resolveOutputBasename,
        generatedSourceFilename,
        generatedHeaderFilename,
        sortGeneratedFilenames,
        resolveGeneratedSourceFile,
        resolveGeneratedHeaderFile,
        isDsPic33AkPart,
        oscillatorManagedFuseFields,
        oscillatorTargetHint,
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
        normalizeFuseDefinitions,
        groupedFuseDefinitions,
        visibleFuseFieldCount,
        derivePinStats,
        deriveDeviceInterfaceInventory,
        extractPeripheralInstance,
    };
}));
