/**
 * Regression tests for the pure CLC schematic routing helpers.
 *
 * The browser renderer builds SVG from these helpers, so covering the
 * grouped-mode stub logic here is enough to catch the dangling-wire
 * regression without bringing a DOM test harness into the repo.
 */
const test = require('node:test');
const assert = require('node:assert/strict');

const schematic = require('../static/app/05-clc-schematic.js');

function segmentOverlapLength(left, right) {
    const leftHorizontal = left[0][1] === left[1][1];
    const rightHorizontal = right[0][1] === right[1][1];

    if (leftHorizontal !== rightHorizontal) {
        return 0;
    }

    if (leftHorizontal) {
        if (left[0][1] !== right[0][1]) {
            return 0;
        }
        const leftMin = Math.min(left[0][0], left[1][0]);
        const leftMax = Math.max(left[0][0], left[1][0]);
        const rightMin = Math.min(right[0][0], right[1][0]);
        const rightMax = Math.max(right[0][0], right[1][0]);
        return Math.max(0, Math.min(leftMax, rightMax) - Math.max(leftMin, rightMin));
    }

    if (left[0][0] !== right[0][0]) {
        return 0;
    }
    const leftMin = Math.min(left[0][1], left[1][1]);
    const leftMax = Math.max(left[0][1], left[1][1]);
    const rightMin = Math.min(right[0][1], right[1][1]);
    const rightMax = Math.max(right[0][1], right[1][1]);
    return Math.max(0, Math.min(leftMax, rightMax) - Math.max(leftMin, rightMin));
}

function countRenderedSegmentOverlaps(routes) {
    let overlaps = 0;
    for (let leftIndex = 0; leftIndex < routes.length; leftIndex++) {
        for (let rightIndex = leftIndex + 1; rightIndex < routes.length; rightIndex++) {
            for (const leftSegment of routes[leftIndex].segments) {
                for (const rightSegment of routes[rightIndex].segments) {
                    if (segmentOverlapLength(leftSegment, rightSegment) > 0) {
                        overlaps += 1;
                    }
                }
            }
        }
    }
    return overlaps;
}

test('buildGateStubSegments omits inactive grouped-mode inputs', () => {
    const segments = schematic.buildGateStubSegments(100, 120, [
        {
            gate: 0,
            laneY: 50,
            pinY: 70,
            active: true,
            cls: 'active-wire',
        },
        {
            gate: 1,
            laneY: 110,
            pinY: 90,
            active: false,
            cls: 'inactive-wire',
        },
    ]);

    assert.deepEqual(segments, [
        {
            cls: 'active-wire',
            points: [[100, 50], [100, 70], [120, 70]],
        },
    ]);
});

test('sourceTraceClass applies stable per-source colors only to active nets', () => {
    assert.equal(
        schematic.sourceTraceClass(0, true),
        'clc-trace-active clc-source-net-1'
    );
    assert.equal(
        schematic.sourceTraceClass(2, true),
        'clc-trace-active clc-source-net-3'
    );
    assert.equal(
        schematic.sourceTraceClass(1, false),
        'clc-trace-dim'
    );
});

test('buildGateStubSegments preserves custom bend channels for multi-input fan-in', () => {
    const segments = schematic.buildGateStubSegments(100, 140, [
        {
            gate: 2,
            laneY: 220,
            pinY: 180,
            channelX: 112,
            active: true,
            cls: 'fan-in-wire',
        },
    ]);

    assert.deepEqual(segments, [
        {
            cls: 'fan-in-wire',
            points: [[100, 220], [112, 220], [112, 180], [140, 180]],
        },
    ]);
});

test('collectEnabledGateInputs keeps true and neg literals distinct for one source', () => {
    const inputs = schematic.collectEnabledGateInputs({
        inputs: [
            { ds: 0, polarity: 'true', enabled: true },
            { ds: 0, polarity: 'neg', enabled: true },
            { ds: 1, polarity: 'true', enabled: false },
            { ds: 1, polarity: 'neg', enabled: false },
            { ds: 2, polarity: 'true', enabled: false },
            { ds: 2, polarity: 'neg', enabled: false },
            { ds: 3, polarity: 'true', enabled: false },
            { ds: 3, polarity: 'neg', enabled: false },
        ],
    }, 110);

    assert.deepEqual(inputs.map(({ ds, polarity }) => ({ ds, polarity })), [
        { ds: 0, polarity: 'true' },
        { ds: 0, polarity: 'neg' },
    ]);
    assert.notEqual(inputs[0].pinY, inputs[1].pinY);
    assert.ok(Number.isInteger(inputs[0].pinY));
    assert.ok(Number.isInteger(inputs[1].pinY));
});

test('collectEnabledGateInputs avoids placing foreign sources on another source lane', () => {
    const inputs = schematic.collectEnabledGateInputs({
        inputs: [
            { ds: 0, polarity: 'true', enabled: true },
            { ds: 0, polarity: 'neg', enabled: false },
            { ds: 1, polarity: 'true', enabled: true },
            { ds: 1, polarity: 'neg', enabled: false },
            { ds: 2, polarity: 'true', enabled: false },
            { ds: 2, polarity: 'neg', enabled: false },
            { ds: 3, polarity: 'true', enabled: true },
            { ds: 3, polarity: 'neg', enabled: false },
        ],
    }, 50);

    const pinBySource = new Map(inputs.map(input => [input.ds, input.pinY]));
    assert.equal(pinBySource.get(0), 50);
    assert.notEqual(pinBySource.get(1), 50);
    assert.notEqual(pinBySource.get(3), 50);
});

test('activeCorePins keeps only gate outputs that are actually present', () => {
    const pins = schematic.activeCorePins({
        'gate-out-0': true,
        'gate-out-1': false,
        'gate-out-2': true,
    }, [
        { gate: 0, x: 10, y: 20 },
        { gate: 1, x: 30, y: 40 },
        { gate: 2, x: 50, y: 60 },
    ]);

    assert.deepEqual(pins, [
        { gate: 0, x: 10, y: 20 },
        { gate: 2, x: 50, y: 60 },
    ]);
});

test('buildFirstStageGrid keeps all lane and pin rows on the routing grid', () => {
    const grid = schematic.buildFirstStageGrid([
        [{ gate: 0, pinY: 35 }, { gate: 1, pinY: 95 }],
        [{ gate: 2, pinY: 205 }, { gate: 3, pinY: 280 }],
        [{ gate: 1, pinY: 110 }, { gate: 2, pinY: 235 }],
        [{ gate: 0, pinY: 65 }, { gate: 1, pinY: 125 }],
    ], schematic.CLC_SVG.LANES, 410);

    assert.ok(grid.columnIndexByValue.has(410));
    for (const laneY of schematic.CLC_SVG.LANES) {
        assert.ok(grid.rowIndexByValue.has(laneY));
    }
    for (const pinY of [35, 65, 95, 125, 205, 235, 280]) {
        assert.ok(grid.rowIndexByValue.has(pinY));
    }
});

test('routeFirstStageNets removes the overlapping-net bug from the screenshot matrix', () => {
    const routed = schematic.routeFirstStageNets([
        [{ gate: 0, pinY: 50 }, { gate: 1, pinY: 95 }],
        [{ gate: 0, pinY: 35 }, { gate: 2, pinY: 205 }, { gate: 3, pinY: 280 }],
        [{ gate: 1, pinY: 110 }, { gate: 2, pinY: 235 }],
        [{ gate: 0, pinY: 65 }, { gate: 1, pinY: 125 }],
    ], schematic.CLC_SVG.LANES, 410);

    assert.equal(schematic.countRoutedNetOverlaps(routed), 0);
    assert.equal(countRenderedSegmentOverlaps(routed.routes), 0);
    assert.ok(Number.isFinite(routed.cost));
});

test('routeFirstStageNets can resolve mutual foreign-lane pressure without overlaps', () => {
    const routed = schematic.routeFirstStageNets([
        [{ gate: 1, pinY: 110 }],
        [{ gate: 0, pinY: 50 }],
        [],
        [],
    ], schematic.CLC_SVG.LANES, 410);

    assert.equal(schematic.countRoutedNetOverlaps(routed), 0);
    assert.equal(countRenderedSegmentOverlaps(routed.routes), 0);
    assert.ok(Number.isFinite(routed.cost));
});

test('routeFirstStageConnection can branch from an existing same-net tree', () => {
    const grid = schematic.buildFirstStageGrid([
        [{ gate: 0, pinY: 50 }, { gate: 1, pinY: 95 }],
        [],
        [],
        [],
    ], schematic.CLC_SVG.LANES, 410);
    const state = (() => {
        const laneYs = schematic.CLC_SVG.LANES;
        const originNodes = laneYs.map((laneY) => (
            grid.width * grid.rowIndexByValue.get(laneY)
        ));
        return {
            edgeOwners: new Map(),
            nodeUsage: new Map(),
            netEdges: Array.from({ length: 4 }, () => new Set()),
            netNodes: originNodes.map((originNode) => new Set([originNode])),
            originNodes,
        };
    })();
    const targetColumnIndex = grid.columnIndexByValue.get(410);
    const firstTarget = (grid.width * grid.rowIndexByValue.get(50)) + targetColumnIndex;
    const secondTarget = (grid.width * grid.rowIndexByValue.get(95)) + targetColumnIndex;

    const firstPath = schematic.routeFirstStageConnection(grid, state, schematic.CLC_SVG.LANES, 0, firstTarget);
    assert.ok(firstPath);
    let previousNode = null;
    for (const nodeId of firstPath.nodes) {
        if (previousNode !== null) {
            state.edgeOwners.set(`${Math.min(previousNode, nodeId)}:${Math.max(previousNode, nodeId)}`, 0);
            state.netEdges[0].add(`${Math.min(previousNode, nodeId)}:${Math.max(previousNode, nodeId)}`);
            state.netNodes[0].add(previousNode);
            state.netNodes[0].add(nodeId);
        }
        previousNode = nodeId;
    }

    const secondPath = schematic.routeFirstStageConnection(grid, state, schematic.CLC_SVG.LANES, 0, secondTarget);
    assert.ok(secondPath);
    assert.ok(secondPath.nodes.some((nodeId) => state.netNodes[0].has(nodeId)));
});

test('chooseAlignedPinY prefers the driver row when spacing allows', () => {
    assert.equal(
        schematic.chooseAlignedPinY(195, 50, 220, [50, 220], 22),
        195
    );
});

test('chooseAlignedPinY nudges to the nearest legal row when blocked', () => {
    assert.equal(
        schematic.chooseAlignedPinY(210, 50, 220, [195, 220], 22),
        173
    );
});

test('scoreOrthogonalPath penalizes vertical detours more than flat runs', () => {
    const flat = schematic.scoreOrthogonalPath([[0, 0], [20, 0]], {
        horizontal: 1,
        vertical: 7,
        bends: 20,
    });
    const detour = schematic.scoreOrthogonalPath([[0, 0], [10, 0], [10, 20], [20, 20]], {
        horizontal: 1,
        vertical: 7,
        bends: 20,
    });

    assert.ok(detour > flat);
});

test('chooseMode2inDffLayout prefers the horizontally aligned D path', () => {
    const layout = schematic.chooseMode2inDffLayout(schematic.CLC_SVG);

    assert.equal(layout.andY, layout.dY);
    assert.equal(layout.andY, schematic.CLC_SVG.CENTER_Y);
});

test('mode 2-in D-FF scored layout beats an off-row candidate', () => {
    const best = schematic.chooseMode2inDffLayout(schematic.CLC_SVG);
    const shifted = schematic.buildMode2inDffLayoutCandidate(
        schematic.CLC_SVG,
        schematic.CLC_SVG.CENTER_Y + 30
    );

    assert.ok(best.score < shifted.score);
});

test('gate geometry exposes deeper input attachment for OR/XOR and farther output tips', () => {
    assert.equal(schematic.gateInputX('and', 100), 100);
    assert.ok(schematic.gateInputX('or', 100) > schematic.gateInputX('and', 100));
    assert.ok(schematic.gateInputX('xor', 100) > schematic.gateInputX('or', 100));
    assert.ok(schematic.gateOutputX('or', 100) > schematic.gateOutputX('and', 100));
    assert.ok(schematic.gateOutputX('xor', 100) > schematic.gateOutputX('or', 100));
});

test('buildBinaryGateInputRoutes attaches to geometry-driven gate pins', () => {
    const gateX = 200;
    const gateY = 160;
    const routes = schematic.buildBinaryGateInputRoutes('xor', gateX, gateY,
        { x: 120, y: 100, cls: 'top' },
        { x: 120, y: 220, cls: 'bottom' });

    const inputX = schematic.gateInputX('xor', gateX);
    assert.deepEqual(routes[0].points[routes[0].points.length - 1], [inputX, gateY - schematic.GATE_PIN_DY]);
    assert.deepEqual(routes[1].points[routes[1].points.length - 1], [inputX, gateY + schematic.GATE_PIN_DY]);
});
