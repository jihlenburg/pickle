/**
 * Verification renderer for progress, notices, and package-diff views.
 *
 * The verification workflow hands structured state into these helpers so the
 * orchestration file can focus on IPC and state transitions instead of HTML
 * assembly.
 */
(function (root, factory) {
    root.PickleVerificationRender = factory(root);
}(typeof globalThis !== 'undefined' ? globalThis : this, function (root) {
    const verificationModel = root.PickleVerificationModel || {};

    function escapeHtml(str) {
        const div = document.createElement('div');
        div.textContent = str;
        return div.innerHTML;
    }

    function renderCorrections(pkg) {
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
            const match = currentPin
                && verificationModel.normalizePad(datasheetPad) === verificationModel.normalizePad(currentPad);
            if (currentPin) totalCompared += 1;
            if (match) matchCount += 1;

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

    function verificationSummaryHtml(packageLabel, isLoaded, alreadyApplied, totalCompared, matchCount) {
        if (!isLoaded && alreadyApplied) {
            return `<div class="verify-summary verify-new-pkg">${escapeHtml(packageLabel)} is already imported as an overlay. Select it from the package list to compare it against the loaded package data.</div>`;
        }
        if (!isLoaded) {
            return '<div class="verify-summary verify-new-pkg">New package \u2014 not in the currently loaded device data. Apply as overlay to use it.</div>';
        }
        if (totalCompared > 0 && matchCount === totalCompared) {
            return `<div class="verify-match">All ${totalCompared} pins match the loaded package data.</div>`;
        }
        if (totalCompared > 0) {
            const diffCount = totalCompared - matchCount;
            return `<div class="verify-summary">${matchCount}/${totalCompared} pins match \u2014 ${diffCount} difference${diffCount > 1 ? 's' : ''} found against the loaded package data.</div>`;
        }
        return '';
    }

    function renderTabs(pkgNames, loadedPackage, result, device) {
        const loadedIdentityKey = verificationModel.packageIdentityForVerification(device, loadedPackage).identityKey;
        const defaultTab = pkgNames.find((name) => verificationModel.packageIdentityForVerification(
            device,
            name,
            result.packages[name],
        ).identityKey === loadedIdentityKey) || pkgNames[0];
        let html = '<div class="verify-pkg-tabs">';
        pkgNames.forEach((name) => {
            const pkg = result.packages[name];
            const entry = verificationModel.packageIdentityForVerification(device, name, pkg);
            const correctionCount = (pkg.corrections || []).length;
            const scoreText = pkg.match_score != null
                ? ` <span class="verify-score ${verificationModel.verificationScoreClass(pkg.match_score)}">${Math.round(pkg.match_score * 100)}%</span>`
                : '';
            const badge = correctionCount > 0 ? ` <span class="verify-corr-badge">${correctionCount}</span>` : '';
            const active = name === defaultTab ? ' active' : '';

            html += `<button class="verify-pkg-tab${active}" data-pkg="${name}">`;
            html += `${escapeHtml(entry.displayName)} (${pkg.pin_count}p)${scoreText}${badge}</button>`;
        });
        html += '</div>';
        return html;
    }

    function renderDetails(pkgNames, loadedPackage, currentPins, result, device) {
        const loadedIdentityKey = verificationModel.packageIdentityForVerification(device, loadedPackage).identityKey;
        const defaultTab = pkgNames.find((name) => verificationModel.packageIdentityForVerification(
            device,
            name,
            result.packages[name],
        ).identityKey === loadedIdentityKey) || pkgNames[0];
        let html = '';

        pkgNames.forEach((name) => {
            const pkg = result.packages[name];
            const entry = verificationModel.packageIdentityForVerification(device, name, pkg);
            const packageLabel = entry.displayName;
            const isLoaded = entry.identityKey === loadedIdentityKey;
            const hidden = name === defaultTab ? '' : ' hidden';
            const { rows, matchCount, totalCompared } = buildVerificationRows(pkg, currentPins, isLoaded);
            const alreadyApplied = !!(
                device?.packages
                && Object.keys(device.packages).some(
                    (packageName) => verificationModel.packageIdentityForVerification(device, packageName).identityKey === entry.identityKey
                )
            );

            html += `<div class="verify-pkg-detail${hidden}" data-pkg="${name}">`;
            if (isLoaded) {
                html += renderCorrections(pkg);
            }
            html += verificationSummaryHtml(packageLabel, isLoaded, alreadyApplied, totalCompared, matchCount);
            html += '<div class="verify-table-wrap"><table class="verify-table"><thead><tr>';
            if (isLoaded) {
                html += '<th>Pin</th><th>Datasheet</th><th>EDC Parser</th><th class="status-icon"></th>';
            } else {
                html += '<th>Pin</th><th colspan="2">Datasheet</th><th></th>';
            }
            html += `</tr></thead><tbody>${rows}</tbody></table></div>`;

            if (alreadyApplied) {
                html += `<button class="verify-apply-btn applied" data-pkg="${name}" disabled>\u2713 ${escapeHtml(packageLabel)} applied</button>`;
            } else {
                html += `<button class="verify-apply-btn" data-pkg="${name}">Apply "${escapeHtml(packageLabel)}" as Overlay</button>`;
            }

            html += '</div>';
        });

        return html;
    }

    function wireResultInteractions(output, onApply) {
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
            button.addEventListener('click', () => onApply(button.dataset.pkg));
        });
    }

    function renderProgress(progress, elapsed) {
        const normalized = verificationModel.normalizeProgress(progress);
        const currentStep = verificationModel.progressStepId(normalized.stage);
        const currentStepIndex = verificationModel.progressSteps.findIndex((step) => step.id === currentStep);
        const percent = Math.max(6, Math.min(100, Math.round(normalized.progress * 100)));

        const stepsHtml = verificationModel.progressSteps.map((step, index) => {
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
        const hint = verificationModel.progressHint(normalized);
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

    function renderTimingNote(device) {
        const clcNote = device?.has_clc
            ? 'CLC input sources can be looked up in a second background pass if they are still missing after pinout verification.'
            : 'This device has no CLC peripheral, so no background CLC lookup will be run.';
        return `
            <div class="verify-expectation">
                pickle trims the datasheet to the pinout pages before upload. ${clcNote}
            </div>`;
    }

    function renderSiblingDatasheetNotice(device, siblingSource) {
        if (!siblingSource || !device) {
            return '';
        }
        return `
            <div class="verify-sibling-notice">
                <strong>Note:</strong> No dedicated datasheet was found for
                <strong>${escapeHtml(device.part_number)}</strong>.
                Verification is using the sibling family datasheet from
                <strong>${escapeHtml(siblingSource)}</strong>
                (same pin-number suffix). Pin assignments should match,
                but double-check against the official datasheet when it
                becomes available.
            </div>`;
    }

    function renderSyntheticPackageNotice(device) {
        if (!device || !root.isSyntheticPackage(device.selected_package)) {
            return '';
        }

        return `
            <div class="verify-synthetic-notice">
                <strong>${escapeHtml(root.displayPackageName(device.selected_package, { long: true }))}</strong> is a fallback package from the EDC, not a real package name. Verify against the datasheet to import the actual package name and pin table.
            </div>`;
    }

    function renderEmptyState(device) {
        return `
            ${renderSyntheticPackageNotice(device)}
            <div class="verify-empty">Load a device and click <strong>Verify Pinout</strong> to cross-check pin assignments against the datasheet.</div>`;
    }

    function renderResultHtml({ device, result, siblingSource }) {
        if (!result?.packages || Object.keys(result.packages).length === 0) {
            return `${renderSyntheticPackageNotice(device)}<div class="verify-error">No package data found in datasheet.</div>`;
        }

        const loadedPackage = device ? (device.selected_package || '') : '';
        const pkgNames = verificationModel.matchingPackages(device, result);
        if (pkgNames.length === 0) {
            return '<div class="verify-error">No matching packages found for this device\'s pin count.</div>';
        }

        let html = '';
        html += renderSiblingDatasheetNotice(device, siblingSource);
        html += renderSyntheticPackageNotice(device);
        html += renderTimingNote(device);
        if (result.notes?.length) {
            html += '<div class="verify-notes">';
            result.notes.forEach((note) => {
                html += `<div class="verify-note">${escapeHtml(note)}</div>`;
            });
            html += '</div>';
        }

        html += renderTabs(pkgNames, loadedPackage, result, device);
        html += renderDetails(pkgNames, loadedPackage, verificationModel.currentPinMap(device), result, device);
        return html;
    }

    return {
        renderProgress,
        renderEmptyState,
        renderResultHtml,
        wireResultInteractions,
    };
}));
