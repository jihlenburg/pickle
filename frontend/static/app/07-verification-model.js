/**
 * Pure verification helpers shared by the verification workflow and renderer.
 *
 * Keeping these functions side-effect free makes the verification UI easier to
 * test and avoids re-encoding stage and package-matching rules in multiple
 * browser files.
 */
(function (root, factory) {
    const api = factory(root);
    if (typeof module !== 'undefined' && module.exports) {
        module.exports = api;
    }
    root.PickleVerificationModel = api;
}(typeof globalThis !== 'undefined' ? globalThis : this, function (root) {
    const packageModel = root.PickleModel || null;
    const PROGRESS_STEPS = [
        { id: 'datasheet', label: 'Get datasheet' },
        { id: 'device', label: 'Load device' },
        { id: 'reduce', label: 'Trim pages' },
        { id: 'upload', label: 'Upload' },
        { id: 'analyze', label: 'AI analysis' },
        { id: 'process', label: 'Process result' },
    ];

    const STAGE_TO_STEP = {
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

    const STAGE_PROGRESS = {
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

    function normalizePad(name) {
        return String(name || '').toUpperCase().replace(/_\d+$/, '');
    }

    function verificationScoreClass(matchScore) {
        if (matchScore >= 0.95) return 'score-good';
        if (matchScore >= 0.8) return 'score-warn';
        return 'score-bad';
    }

    function currentPinMap(device) {
        const pins = {};
        if (device?.pins) {
            device.pins.forEach((pin) => {
                pins[pin.position] = pin;
            });
        }
        return pins;
    }

    function packageIdentityForVerification(device, packageName, packageData = null) {
        const existingMeta = device?.packages?.[packageName] || null;
        const mergedMeta = {
            ...(existingMeta || {}),
            ...(packageData || {}),
        };

        if (packageModel && typeof packageModel.packageIdentity === 'function') {
            return packageModel.packageIdentity(packageName, mergedMeta);
        }

        const normalizedName = String(packageName || '').trim();
        const pinCount = Number(mergedMeta?.pin_count);
        return {
            backendKey: normalizedName,
            displayName: normalizedName || '—',
            identityKey: `${Number.isFinite(pinCount) ? pinCount : 0}|${normalizedName.toUpperCase()}`,
        };
    }

    function matchingPackages(device, result) {
        const devicePinCount = device ? device.pin_count : 0;
        return Object.keys(result?.packages || {}).filter((name) => {
            const pkg = result.packages[name];
            return !devicePinCount || pkg.pin_count === devicePinCount;
        });
    }

    function normalizeProgress(payload) {
        if (payload && typeof payload === 'object' && !Array.isArray(payload)) {
            const stage = String(payload.stage || 'legacy');
            return {
                stage,
                label: String(payload.label || payload.message || 'Working...'),
                detail: payload.detail ? String(payload.detail) : '',
                progress: typeof payload.progress === 'number'
                    ? payload.progress
                    : (STAGE_PROGRESS[stage] || 0.1),
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

    function progressHint(progress) {
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

    function progressStepId(stage) {
        return STAGE_TO_STEP[stage] || 'datasheet';
    }

    return {
        progressSteps: PROGRESS_STEPS,
        normalizePad,
        verificationScoreClass,
        currentPinMap,
        packageIdentityForVerification,
        matchingPackages,
        normalizeProgress,
        progressHint,
        progressStepId,
    };
}));
