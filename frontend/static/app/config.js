/**
 * Unified frontend configuration.
 *
 * Owns theme tokens, typography, and UI constants that should remain data,
 * not logic, so future polish passes can update one maintained source.
 */
(function (root, factory) {
    const api = factory();
    if (typeof module !== 'undefined' && module.exports) {
        module.exports = api;
    }
    root.PickleConfig = api;
}(typeof globalThis !== 'undefined' ? globalThis : this, function () {
    function deepFreeze(value) {
        if (!value || typeof value !== 'object' || Object.isFrozen(value)) {
            return value;
        }
        Object.getOwnPropertyNames(value).forEach((key) => {
            deepFreeze(value[key]);
        });
        return Object.freeze(value);
    }

    function format(template, values) {
        return String(template).replace(/\{([a-zA-Z0-9_]+)\}/g, (_, key) => (
            Object.prototype.hasOwnProperty.call(values, key) ? String(values[key]) : ''
        ));
    }

    const config = deepFreeze({
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
        theme: {
            modes: ['dark', 'light', 'system'],
            mediaQuery: '(prefers-color-scheme: light)',
            cycle: {
                dark: 'light',
                light: 'system',
                system: 'dark',
            },
            labels: {
                dark: 'Dark',
                light: 'Light',
                system: 'System',
            },
        },
        typography: {
            body: '"Avenir Next", "IBM Plex Sans", "Segoe UI", sans-serif',
            mono: '"JetBrains Mono", "Fira Code", "Cascadia Code", monospace',
        },
        ui: {
            undo: {
                maxSnapshots: 50,
            },
            deviceLoad: {
                cachedStatus: 'Loading...',
                remoteStatus: 'Downloading DFP pack...',
            },
            partPicker: {
                maxSuggestions: 8,
                cachedLabel: 'cached',
            },
            welcomeIntro: {
                eyebrow: 'Native dsPIC33 + PIC24 pin planning',
                title: 'Load a device, verify the real package, and generate clean init code.',
                description: 'pickle keeps Microchip device-pack parsing, datasheet-backed package verification, fuse setup, and code generation in one desktop workspace.',
                primaryActionLabel: 'Start With a Part',
                secondaryActionLabel: 'Open Existing Config',
                dismissActionLabel: 'Skip Intro',
                helperText: 'Settings controls theme, verifier provider, startup behavior, and compiler defaults.',
                featureCards: [
                    {
                        title: 'Pull the right device pack',
                        body: 'Type a part number and pickle loads the matching Microchip DFP, even when it is not cached locally yet.',
                    },
                    {
                        title: 'Verify against the datasheet',
                        body: 'Import real package tables and pinout overlays from the datasheet so the UI stops depending on vague fallback package names.',
                    },
                    {
                        title: 'Stay in one editing loop',
                        body: 'Pin assignments, peripheral routing, fuses, CLC setup, and generated init code all stay connected in the same session.',
                    },
                ],
                sampleParts: [
                    {
                        label: 'dsPIC33CK64MP102',
                        note: 'Compact motor-control baseline',
                        part: 'DSPIC33CK64MP102',
                    },
                    {
                        label: 'dsPIC33AK64MC105',
                        note: 'AK family MC device',
                        part: 'DSPIC33AK64MC105',
                    },
                    {
                        label: 'dsPIC33AK256MPS205',
                        note: 'AK family MPS device',
                        part: 'DSPIC33AK256MPS205',
                    },
                ],
            },
            timings: {
                buttonFlashMs: 1500,
                pinScrollHighlightMs: 1500,
                packageDiagramDebounceMs: 200,
                tooltipDelayMs: 350,
            },
            catalog: {
                ageFractionDigits: 1,
                labels: {
                    fresh: 'fresh',
                    stale: 'stale',
                    offline: 'offline',
                },
                ageUnknown: 'age unknown',
                ageFormat: '{hours}h old',
                badgeText: '{total} devices | {cached} cached | {freshness}',
                availableTitle: 'Device catalog is {freshness}. {ageText}. Click to refresh from Microchip.',
                unavailableTitle: 'Device catalog not yet available. Click to fetch from Microchip.',
                refreshingBadge: 'Refreshing...',
                refreshingStatus: 'Refreshing device catalog...',
                refreshFailedBadge: 'Refresh failed',
                refreshFailedStatus: 'Error: failed to refresh device catalog',
                refreshedStatus: 'Catalog refreshed: {deviceCount} devices across {packCount} packs',
            },
            compiler: {
                familyLabels: {
                    pic24: 'PIC24',
                    dspic33: 'dsPIC33',
                    unknown: 'device',
                },
                buttonLabel: 'Check with {compiler}',
                buttonFallbackLabel: 'Compiler Check',
                checkingStatus: 'Compiling...',
                successMessage: '{compiler}: compiled successfully — no errors or warnings',
                warningMessage: '{compiler}: compiled with warnings:\n{details}',
                failureMessage: '{compiler}: compilation failed:\n{details}',
            },
            fuses: {
                oscillatorManagedNote: 'Managed by the oscillator configuration above.',
                groups: {
                    clock: 'Clock / Oscillator',
                    debug: 'Debug / Programming',
                    watchdog: 'Watchdog',
                    bootSecurity: 'Boot / Security',
                    protection: 'Flash / Code Protection',
                    integrity: 'Integrity / Safety',
                    device: 'Device Options',
                    other: 'Other Configuration',
                },
            },
            configFiles: {
                unsavedName: 'Unsaved config',
                saveButton: 'Save Config',
                saveAsButton: 'Save As...',
                renameButton: 'Rename...',
                moreActionsTitle: 'More save actions',
                saveDialogTitle: 'Save Pin Configuration',
                saveAsDialogTitle: 'Save Pin Configuration As',
                renameDialogTitle: 'Rename Pin Configuration',
                openDialogTitle: 'Open Pin Configuration',
                saveShortcutHint: 'Save current configuration (Ctrl/Cmd+S)',
            },
            packageManager: {
                menuButtonLabel: '...',
                menuButtonTitle: 'Package actions',
                menuEditLabel: 'Edit Name...',
                menuResetLabel: 'Reset Name',
                menuDeleteLabel: 'Delete Overlay',
                dialogTitle: 'Edit Package Name',
                currentLabel: 'Shown in UI',
                storedLabel: 'Backend key',
                sourceLabel: 'Source',
                nameLabel: 'Displayed package name',
                namePlaceholder: 'Enter the name to show in the UI',
                saveButton: 'Save Name',
                resetButton: 'Reset Name',
                deleteButton: 'Delete Overlay',
                closeButton: 'Close',
                overlayNote: 'Changing the displayed name here only changes the UI label. The stored backend key stays stable, and overlay-backed packages can also be deleted here.',
                builtinNote: 'This package comes from the device pack. You can change the displayed name here without changing the underlying backend package key.',
                sourceOverlay: 'Overlay package',
                sourceBuiltin: 'Built-in device-pack package',
                deleteConfirm: 'Delete overlay package "{packageName}"?',
                savedStatus: 'Updated displayed package name to {packageName}',
                resetStatus: 'Reset the displayed package name for {packageName}',
                deletedStatus: 'Deleted overlay package {packageName}',
            },
        },
        themes: {
            dark: {
                '--font-body': '"Avenir Next", "IBM Plex Sans", "Segoe UI", sans-serif',
                '--font-mono': '"JetBrains Mono", "Fira Code", "Cascadia Code", monospace',
                '--bg': '#0b1220',
                '--bg-card': '#121a2c',
                '--bg-pin': '#16233a',
                '--bg-pin-assigned': '#122b26',
                '--text': '#ebf2ff',
                '--text-dim': '#95a3bf',
                '--text-inverse': '#ffffff',
                '--accent': '#ff6b6b',
                '--accent-gradient-end': '#ff8f6b',
                '--accent2': '#4dd4c6',
                '--accent2-gradient-end': '#1fa7d0',
                '--accent2-rgb': '77, 212, 198',
                '--border': 'rgba(129, 151, 191, 0.16)',
                '--uart': '#e6a23c',
                '--spi': '#67c23a',
                '--i2c': '#409eff',
                '--pwm': '#f56c6c',
                '--timer': '#909399',
                '--adc': '#b37feb',
                '--cmp': '#2dd4bf',
                '--opamp': '#f472b6',
                '--dac': '#a78bfa',
                '--gpio': '#64748b',
                '--int': '#fbbf24',
                '--sys': '#5eead4',
                '--other': '#00adb5',
                '--icsp': '#f0c040',
                '--jtag': '#ff9f43',
                '--selected': 'rgba(255, 200, 50, 0.18)',
                '--selected-border': 'rgba(255, 200, 50, 0.5)',
                '--code-bg': '#09111f',
                '--code-fg': '#dce7fb',
                '--hover-overlay': 'rgba(255,255,255,0.04)',
                '--conflict-bg': 'rgba(255,107,107,0.08)',
                '--conflict-flash': 'rgba(255,107,107,0.55)',
                '--conflict-bg2': 'rgba(255,107,107,0.28)',
                '--conflict-border': 'rgba(255,107,107,0.18)',
                '--assigned-overlay': 'rgba(77,212,198,0.2)',
                '--rp-bg': 'rgba(77,212,198,0.15)',
                '--icsp-bg': 'rgba(240,192,64,0.15)',
                '--jtag-bg': 'rgba(255,159,67,0.16)',
                '--success-bg': 'rgba(103,194,58,0.15)',
                '--status-good': '#67c23a',
                '--status-warn': '#e6a23c',
                '--surface-strong': 'rgba(14, 21, 36, 0.96)',
                '--shadow': '0 8px 24px rgba(3, 7, 18, 0.3)',
                '--chip-bg': 'rgba(255,255,255,0.04)',
                '--chip-border': 'rgba(255,255,255,0.08)',
                '--pin-scroll-highlight': 'rgba(77,212,198,0.15)',
                '--clc-source-1': '#67e8f9',
                '--clc-source-2': '#fbbf24',
                '--clc-source-3': '#c084fc',
                '--clc-source-4': '#fb7185',
            },
            light: {
                '--font-body': '"Avenir Next", "IBM Plex Sans", "Segoe UI", sans-serif',
                '--font-mono': '"JetBrains Mono", "Fira Code", "Cascadia Code", monospace',
                '--bg': '#edf2f8',
                '--bg-card': 'rgba(255,255,255,0.88)',
                '--bg-pin': '#edf4ff',
                '--bg-pin-assigned': '#effcf8',
                '--text': '#162033',
                '--text-dim': '#6d7890',
                '--text-inverse': '#ffffff',
                '--accent': '#d9485f',
                '--accent-gradient-end': '#ff8f6b',
                '--accent2': '#0f9b8e',
                '--accent2-gradient-end': '#1fa7d0',
                '--accent2-rgb': '15, 155, 142',
                '--border': 'rgba(22, 32, 51, 0.12)',
                '--uart': '#c2790e',
                '--spi': '#3d8b2b',
                '--i2c': '#2563eb',
                '--pwm': '#dc2626',
                '--timer': '#6b7280',
                '--adc': '#7c3aed',
                '--cmp': '#0d9488',
                '--opamp': '#db2777',
                '--dac': '#6d28d9',
                '--gpio': '#475569',
                '--int': '#d97706',
                '--sys': '#0d9488',
                '--other': '#0891b2',
                '--icsp': '#b88a00',
                '--jtag': '#c96a00',
                '--selected': 'rgba(180, 140, 0, 0.12)',
                '--selected-border': 'rgba(180, 140, 0, 0.4)',
                '--code-bg': '#f8fbff',
                '--code-fg': '#162033',
                '--hover-overlay': 'rgba(15, 23, 42, 0.04)',
                '--conflict-bg': 'rgba(217,72,95,0.06)',
                '--conflict-flash': 'rgba(217,72,95,0.28)',
                '--conflict-bg2': 'rgba(217,72,95,0.15)',
                '--conflict-border': 'rgba(217,72,95,0.14)',
                '--assigned-overlay': 'rgba(15,155,142,0.1)',
                '--rp-bg': 'rgba(15,155,142,0.1)',
                '--icsp-bg': 'rgba(184,138,0,0.1)',
                '--jtag-bg': 'rgba(201,106,0,0.1)',
                '--success-bg': 'rgba(61,139,43,0.1)',
                '--status-good': '#3d8b2b',
                '--status-warn': '#c2790e',
                '--surface-strong': 'rgba(255,255,255,0.96)',
                '--shadow': '0 8px 20px rgba(80, 96, 128, 0.1)',
                '--chip-bg': 'rgba(15, 23, 42, 0.04)',
                '--chip-border': 'rgba(15, 23, 42, 0.08)',
                '--pin-scroll-highlight': 'rgba(15,155,142,0.1)',
                '--clc-source-1': '#0891b2',
                '--clc-source-2': '#b45309',
                '--clc-source-3': '#7c3aed',
                '--clc-source-4': '#e11d48',
            },
        },
    });

    function themeTokens(mode) {
        return config.themes[mode] || config.themes[config.defaults.themeMode];
    }

    function applyDocumentTheme(doc, mode) {
        if (!doc || !doc.documentElement) {
            return;
        }
        const theme = themeTokens(mode);
        doc.documentElement.setAttribute('data-theme', mode);
        doc.documentElement.style.colorScheme = mode === 'light' ? 'light' : 'dark';
        for (const [name, value] of Object.entries(theme)) {
            doc.documentElement.style.setProperty(name, value);
        }
    }

    return {
        ...config,
        format,
        themeTokens,
        applyDocumentTheme,
    };
}));
