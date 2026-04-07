/**
 * Pure reservation and conflict policy helpers.
 *
 * Keeps regex- and rule-heavy hardware policy logic separate from the runtime
 * DOM orchestration in `01-reservations.js`, which makes the policy easier to
 * test and evolve without dragging along UI behavior.
 */
(function (root, factory) {
    const api = factory();
    if (typeof module !== 'undefined' && module.exports) {
        module.exports = api;
    }
    root.PickleReservationPolicy = api;
}(typeof globalThis !== 'undefined' ? globalThis : this, function () {
    function isI2cRoutingFunction(fn) {
        return /^(A?SCL[12]|A?SDA[12])$/.test(fn);
    }

    function parseI2cRoutingFunction(fn) {
        const match = String(fn || '').match(/^(A?)(SCL|SDA)([12])$/);
        if (!match) return null;
        return {
            alternate: Boolean(match[1]),
            role: match[2],
            channel: parseInt(match[3], 10),
        };
    }

    function getI2cRoutingFunctionName(channel, role, alternate) {
        return `${alternate ? 'A' : ''}${role}${channel}`;
    }

    function normalizeIcspPair(pair) {
        const numericPair = Number(pair);
        return Number.isInteger(numericPair) && numericPair > 0 ? numericPair : null;
    }

    function isIcspFunctionForPair(fn, pair) {
        const normalizedName = String(fn || '').trim().toUpperCase();
        if (normalizedName === 'MCLR') {
            return true;
        }

        const normalizedPair = normalizeIcspPair(pair);
        if (normalizedPair === null) {
            return false;
        }

        return new RegExp(`^PGC${normalizedPair}$|^PGD${normalizedPair}$|^PGEC${normalizedPair}$|^PGED${normalizedPair}$`)
            .test(normalizedName);
    }

    function isPinInIcspPair(pin, pair) {
        return Boolean(pin?.functions?.some((fn) => isIcspFunctionForPair(fn, pair)));
    }

    function isJtagFunction(fn) {
        return /^(TCK|TMS|TDI|TDO)$/.test(fn);
    }

    function normalizeAssignmentsForPin(value) {
        if (!value) return [];
        return Array.isArray(value) ? value.filter(Boolean) : [value];
    }

    function forEachAssignedPin(assignments, callback) {
        for (const [pinPos, value] of Object.entries(assignments || {})) {
            const list = normalizeAssignmentsForPin(value);
            if (list.length === 0) continue;
            callback(parseInt(pinPos, 10), list);
        }
    }

    function analyzeAssignmentConflicts(assignments, helpers) {
        const conflicts = [];
        const conflictPins = new Set();
        const used = {};

        forEachAssignedPin(assignments, (pinPos, list) => {
            for (const assign of list) {
                const key = `${assign.peripheral}_${assign.direction}`;
                if (used[key]) {
                    conflicts.push(
                        `${assign.peripheral} (${assign.direction}) assigned to both pin ${used[key]} and pin ${pinPos}`
                    );
                    conflictPins.add(parseInt(used[key], 10));
                    conflictPins.add(pinPos);
                } else {
                    used[key] = pinPos;
                }
            }
        });

        forEachAssignedPin(assignments, (pinPos, list) => {
            if (list.length < 2) return;

            const hasDigital = list.some((a) => !helpers.isAnalogFunction(a.peripheral));
            const hasAnalog = list.some((a) => helpers.isAnalogFunction(a.peripheral));

            if (hasDigital && hasAnalog) {
                const digitalNames = list
                    .filter((a) => !helpers.isAnalogFunction(a.peripheral))
                    .map((a) => a.peripheral);
                const analogNames = list
                    .filter((a) => helpers.isAnalogFunction(a.peripheral))
                    .map((a) => a.peripheral);
                conflicts.push(
                    `Pin ${pinPos}: analog/digital conflict — ${analogNames.join(', ')} vs ${digitalNames.join(', ')}`
                );
                conflictPins.add(pinPos);
            }

            const analogOutputs = list.filter((a) => helpers.isAnalogOutput(a.peripheral));
            if (analogOutputs.length > 1) {
                conflicts.push(
                    `Pin ${pinPos}: multiple analog outputs — ${analogOutputs.map((a) => a.peripheral).join(', ')}`
                );
                conflictPins.add(pinPos);
            }
        });

        return {
            messages: conflicts,
            conflictPins,
        };
    }

    return {
        isI2cRoutingFunction,
        parseI2cRoutingFunction,
        getI2cRoutingFunctionName,
        isIcspFunctionForPair,
        isPinInIcspPair,
        isJtagFunction,
        analyzeAssignmentConflicts,
    };
}));
