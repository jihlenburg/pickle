/**
 * Shared pin-presentation helpers for the left-panel views.
 *
 * These helpers keep the pin-table, package diagram, and peripheral cards
 * aligned on the same derived view state instead of re-deriving labels and
 * reservation flags independently in each renderer.
 */
(function (root, factory) {
    const api = factory();
    if (typeof module !== 'undefined' && module.exports) {
        module.exports = api;
    }
    root.PickleViewModel = api;
}(typeof globalThis !== 'undefined' ? globalThis : this, function () {
    function pinPortLabel(pin, fallback = '—') {
        if (!pin) return fallback;
        if (pin.port) {
            return `R${pin.port}${pin.port_bit}`;
        }
        return pin.pad_name || pin.functions?.[0] || fallback;
    }

    function buildPinPresentation(pin, context) {
        const assignmentsAt = context.getAssignmentsAt(pin.position);
        const assigned = assignmentsAt.length > 0;
        const signalName = context.signalNames?.[pin.position] || '';
        const icsp = Boolean(context.isIcspPin(pin));
        const jtag = Boolean(context.isJtagPin(pin));
        const portLabel = pinPortLabel(pin);

        let packageLabel = portLabel;
        if (assigned) {
            packageLabel = signalName || assignmentsAt.map((assignment) => assignment.peripheral).join(', ');
        } else if (jtag) {
            packageLabel = context.getJtagFunction(pin) || 'JTAG';
        }

        return {
            assignmentsAt,
            assigned,
            signalName,
            icsp,
            jtag,
            blocked: icsp || jtag,
            portLabel,
            packageLabel,
        };
    }

    function signalNameForAssignedPeripheral(signalName, reverseAssignments, signalNames) {
        const pinPos = reverseAssignments?.[signalName];
        if (pinPos === undefined) {
            return '';
        }
        return signalNames?.[pinPos] || '';
    }

    return {
        pinPortLabel,
        buildPinPresentation,
        signalNameForAssignedPeripheral,
    };
}));
