/**
 * Shared mutation helpers for interactive editor state.
 *
 * Centralizes the repetitive post-edit bookkeeping needed across the pin,
 * peripheral, fuse, and CLC editors so individual handlers can focus on the
 * actual state change instead of reimplementing undo, redraw, conflict, and
 * dirty-state flows.
 */

function renderCurrentEditorView() {
    if (typeof renderActiveView === 'function') {
        renderActiveView();
        return;
    }
    renderDevice();
}

function renderLeftPanelViewFrame() {
    renderDeviceSummary();
    renderPackageDiagram();
}

function finalizeLeftPanelViewFrame() {
    updateSummary();
    checkConflicts();
}

function syncAssignedPinRowState(pinPos) {
    if (!Number.isInteger(pinPos)) {
        return;
    }
    const row = document.getElementById(`pin-row-${pinPos}`);
    if (row) {
        row.classList.toggle('assigned', !!assignments[pinPos]);
    }
}

function refreshClcDerivedViews() {
    updateClcRegisters();
    renderClcModuleTabs();
    if (typeof renderClcSchematic === 'function') {
        renderClcSchematic();
    }
}

function applyEditorMutation(mutate, options = {}) {
    const {
        snapshot = false,
        updatePinRow = null,
        updateFuncTags = null,
        updateSummaryMetrics = false,
        refreshConflicts = false,
        renderPackage = false,
        renderPeripheral = false,
        renderActive = false,
        refreshClc = false,
        markDirty = true,
    } = options;

    if (snapshot) {
        pushUndo();
    }

    mutate();

    if (renderPeripheral) {
        renderPeripheralView();
    } else if (renderActive) {
        renderCurrentEditorView();
    } else {
        syncAssignedPinRowState(updatePinRow);
        if (Number.isInteger(updateFuncTags)) {
            updateFuncTagStates(updateFuncTags);
        }
        if (updateSummaryMetrics) {
            updateSummary();
        }
        if (refreshConflicts) {
            checkConflicts();
        }
        if (renderPackage) {
            renderPackageDiagram();
        }
    }

    if (refreshClc) {
        refreshClcDerivedViews();
    }

    if (markDirty) {
        markConfigDocumentDirty();
    }
}

function applyPinAssignmentMutation(pinPos, mutate, options = {}) {
    applyEditorMutation(mutate, {
        snapshot: options.snapshot ?? true,
        updatePinRow: pinPos,
        updateFuncTags: pinPos,
        updateSummaryMetrics: true,
        refreshConflicts: true,
        renderPackage: true,
        markDirty: options.markDirty ?? true,
    });
}

function applyPeripheralAssignmentMutation(mutate, options = {}) {
    applyEditorMutation(mutate, {
        snapshot: options.snapshot ?? true,
        renderPeripheral: true,
        markDirty: options.markDirty ?? true,
    });
}

function applySignalNameMutation(mutate, options = {}) {
    applyEditorMutation(mutate, {
        snapshot: options.snapshot ?? false,
        renderPackage: options.renderPackage ?? true,
        renderPeripheral: options.renderPeripheral ?? false,
        markDirty: options.markDirty ?? true,
    });
}

function applyClcMutation(mutate, options = {}) {
    applyEditorMutation(mutate, {
        snapshot: options.snapshot ?? false,
        refreshClc: true,
        markDirty: options.markDirty ?? true,
    });
}
