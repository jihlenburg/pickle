/**
 * CLC schematic SVG renderer.
 *
 * Generates a datasheet-style logic diagram from the CLC module config.
 * Uses a fixed left-to-right lane-based layout with deterministic
 * orthogonal routing — no auto-layout, no external libraries.
 *
 * Rendering pipeline:
 *   1. buildClcSemanticModel()  — config → structured description
 *   2. resolveActiveTraces()    — propagate activity flags
 *   3. renderClcSchematic()     — build/update SVG DOM
 *
 * Depends on globals from 05-clc-designer.js:
 *   clcConfig, clcActiveModule, getClcSourceLabel, CLC_MODES
 */

// =============================================================================
// Layout constants
// =============================================================================

const CLC_SVG = Object.freeze({
    W: 960, H: 340,

    // Column X positions
    COL_LABEL_X:  90,       // source label right edge
    COL_MUX_X:   115,       // MUX box left edge
    COL_MUX_W:    50,       // MUX box width
    COL_RAIL_X:  180,       // data rail start
    COL_GATE_X:  390,       // gate symbol left edge
    COL_GATE_OUT: 480,      // gate output X (after symbol body)
    COL_CORE_X:  540,       // mode core left edge
    COL_CORE_OUT: 700,      // mode core output X
    COL_COND_X:  730,       // output conditioning start
    COL_PIN_X:   910,       // final output pin

    // Row Y centers — grouped: lanes 1-2 close together, lanes 3-4 close
    // together, wider gap between the two groups.
    LANES: [50, 110, 220, 280],
    CENTER_Y: 165,          // vertical center for mode core + output

    // Group centers (midpoint of each pair) — used by mode core renderers
    GROUP1_Y: 80,           // (50+110)/2
    GROUP2_Y: 250,          // (220+280)/2

    // Gate-to-core vertical routing channels (spread evenly)
    CHAN_X: [496, 506, 516, 526],

    // FF/latch box
    FF_W: 76,
    FF_H: 190,

    // Styling
    WIRE_W:   1.0,
    SYM_W:    1.5,
    BUBBLE_R: 4,
    // Junction dots need to survive the downscaled preview panel. Smaller
    // values collapse into sub-pixel specks and make real tees look missing.
    DOT_R:    4,
    FONT_LABEL: 11,
    FONT_PIN:   10,
    FONT_TITLE: 11,
});

// =============================================================================
// SVG helpers
// =============================================================================

const SVG_NS = 'http://www.w3.org/2000/svg';

function svgEl(tag, attrs) {
    const el = document.createElementNS(SVG_NS, tag);
    if (attrs) {
        for (const [k, v] of Object.entries(attrs)) {
            el.setAttribute(k, v);
        }
    }
    return el;
}

function svgLine(x1, y1, x2, y2, cls) {
    return svgEl('line', {
        x1, y1, x2, y2,
        class: cls || '',
        'vector-effect': 'non-scaling-stroke',
    });
}

function svgPolyline(points, cls) {
    return svgEl('polyline', {
        points: points.map(p => p.join(',')).join(' '),
        fill: 'none',
        class: cls || '',
        'vector-effect': 'non-scaling-stroke',
    });
}

function svgText(x, y, text, cls, anchor) {
    const el = svgEl('text', {
        x, y,
        class: cls || '',
        'text-anchor': anchor || 'start',
        'dominant-baseline': 'central',
    });
    el.textContent = text;
    return el;
}

function svgRect(x, y, w, h, cls, rx, fill) {
    return svgEl('rect', {
        x, y, width: w, height: h, rx: rx || 3,
        class: cls || '',
        style: 'fill: ' + (fill || 'var(--bg)'),
        'vector-effect': 'non-scaling-stroke',
    });
}

function svgCircle(cx, cy, r, cls) {
    return svgEl('circle', {
        cx, cy, r,
        class: cls || '',
        fill: 'none',
        'vector-effect': 'non-scaling-stroke',
    });
}

function svgDot(cx, cy, cls) {
    return svgEl('circle', {
        cx, cy, r: CLC_SVG.DOT_R,
        class: (cls || '') + ' clc-dot',
        'vector-effect': 'non-scaling-stroke',
    });
}

function traceClass(active) {
    return active ? 'clc-trace-active' : 'clc-trace-dim';
}

function sourceTraceClass(sourceIndex, active) {
    const base = traceClass(active);
    return active ? `${base} clc-source-net-${sourceIndex + 1}` : base;
}

// =============================================================================
// Stage 1: Semantic model
// =============================================================================

function buildClcSemanticModel(mod) {
    const sources = [];
    const gates = [];

    // Determine which data sources are tapped by any gate
    const sourceActive = [false, false, false, false];
    for (let g = 0; g < 4; g++) {
        for (let d = 0; d < 4; d++) {
            const tIdx = d * 2;
            const nIdx = d * 2 + 1;
            if (mod.gates[g][tIdx] || mod.gates[g][nIdx]) {
                sourceActive[d] = true;
            }
        }
    }

    for (let d = 0; d < 4; d++) {
        sources.push({
            index: d,
            mux: mod.ds[d],
            label: getClcSourceLabel(d, mod.ds[d]),
            active: sourceActive[d],
        });
    }

    for (let g = 0; g < 4; g++) {
        const inputs = [];
        let anyEnabled = false;
        for (let d = 0; d < 4; d++) {
            const tIdx = d * 2;
            const nIdx = d * 2 + 1;
            const tEnabled = mod.gates[g][tIdx];
            const nEnabled = mod.gates[g][nIdx];
            inputs.push({ ds: d, polarity: 'true', enabled: tEnabled });
            inputs.push({ ds: d, polarity: 'neg',  enabled: nEnabled });
            if (tEnabled || nEnabled) anyEnabled = true;
        }
        gates.push({
            index: g,
            inputs,
            inverted: mod.gpol[g],
            active: anyEnabled,
        });
    }

    const anyGateActive = gates.some(g => g.active);

    return {
        sources,
        gates,
        mode: {
            value: mod.mode,
            name: CLC_MODES[mod.mode].name,
        },
        output: {
            lcpol: mod.lcpol,
            lcoe: mod.lcoe,
            lcen: mod.lcen,
            intp: mod.intp,
            intn: mod.intn,
            active: anyGateActive && mod.lcen,
        },
    };
}

// =============================================================================
// Stage 2: Trace activity
// =============================================================================

function resolveActiveTraces(sem) {
    const t = {};
    for (let d = 0; d < 4; d++) {
        t['src-' + d] = sem.sources[d].active;
        t['rail-' + d] = sem.sources[d].active;
    }
    for (let g = 0; g < 4; g++) {
        t['gate-' + g] = sem.gates[g].active;
        t['gate-out-' + g] = sem.gates[g].active;
        for (let i = 0; i < 8; i++) {
            t['gate-' + g + '-in-' + i] = sem.gates[g].inputs[i].enabled;
        }
    }
    t['core-out'] = sem.output.active;
    t['output'] = sem.output.active;
    return t;
}

// =============================================================================
// Column renderers
// =============================================================================

/** Column A+B: Source labels and MUX boxes */
function renderSources(layers, sem, traces) {
    const S = CLC_SVG;
    for (let d = 0; d < 4; d++) {
        const y = S.LANES[d];
        const cls = traceClass(traces['src-' + d]);
        const wireCls = sourceTraceClass(d, traces['src-' + d]);

        // MUX box
        const mux = svgRect(S.COL_MUX_X, y - 18, S.COL_MUX_W, 36, cls + ' clc-sym', 4);
        layers.symbols.appendChild(mux);

        // MUX label inside box
        const muxLabel = svgText(S.COL_MUX_X + S.COL_MUX_W / 2, y, 'DS' + (d + 1),
            cls + ' clc-label-pin', 'middle');
        layers.labels.appendChild(muxLabel);

        // Source name to the left
        const srcLabel = svgText(S.COL_LABEL_X, y, sem.sources[d].label,
            cls + ' clc-label-src', 'end');
        layers.labels.appendChild(srcLabel);

        // Wire from source label to MUX input
        const wireIn = svgLine(S.COL_LABEL_X + 4, y, S.COL_MUX_X, y, wireCls + ' clc-wire');
        layers.wires.appendChild(wireIn);

        // Wire from MUX output to rail start
        const wireOut = svgLine(
            S.COL_MUX_X + S.COL_MUX_W,
            y,
            S.COL_RAIL_X,
            y,
            wireCls + ' clc-wire'
        );
        layers.wires.appendChild(wireOut);
    }
}

const GATE_SC = 1.2;
const GATE_RENDER = Object.freeze({
    scale: GATE_SC,
    bodyW: 33 * GATE_SC,
    bodyH: 30 * GATE_SC,
    symX: CLC_SVG.COL_GATE_X + 20,
});

const FIRST_STAGE_ROUTE = Object.freeze({
    columnStep: 10,
    rowStep: 10,
    rowMargin: 30,
    bendPenalty: 16,
    crossingPenalty: 42,
    verticalWeight: 1.8,
    reuseWeight: 0.08,
    foreignLanePenalty: 12,
});

function snapCoord(value, step) {
    const snapStep = step || 1;
    return Math.round(value / snapStep) * snapStep;
}

function uniqueSortedNumbers(values) {
    return [...new Set(values)].sort((left, right) => left - right);
}

function rangeNumbers(start, end, step) {
    const values = [];
    for (let value = start; value <= end; value += step) {
        values.push(value);
    }
    return values;
}

function buildFirstStageGrid(sourceConnections, laneYs, targetX) {
    const targetPinYs = sourceConnections
        .flat()
        .map((target) => target.pinY);
    const minY = Math.max(
        10,
        Math.min(...laneYs, ...(targetPinYs.length ? targetPinYs : laneYs)) - FIRST_STAGE_ROUTE.rowMargin
    );
    const maxY = Math.min(
        CLC_SVG.H - 10,
        Math.max(...laneYs, ...(targetPinYs.length ? targetPinYs : laneYs)) + FIRST_STAGE_ROUTE.rowMargin
    );
    const rows = uniqueSortedNumbers([
        ...laneYs,
        ...targetPinYs,
        ...rangeNumbers(minY, maxY, FIRST_STAGE_ROUTE.rowStep),
    ]);
    const columns = uniqueSortedNumbers([
        ...rangeNumbers(CLC_SVG.COL_RAIL_X, targetX, FIRST_STAGE_ROUTE.columnStep),
        targetX,
    ]);
    const rowIndexByValue = new Map(rows.map((value, index) => [value, index]));
    const columnIndexByValue = new Map(columns.map((value, index) => [value, index]));

    return {
        rows,
        columns,
        rowIndexByValue,
        columnIndexByValue,
        width: columns.length,
    };
}

function routingNodeId(grid, xIndex, yIndex) {
    return (yIndex * grid.width) + xIndex;
}

function routingNodeCoords(grid, nodeId) {
    const xIndex = nodeId % grid.width;
    const yIndex = Math.floor(nodeId / grid.width);
    return {
        nodeId,
        xIndex,
        yIndex,
        x: grid.columns[xIndex],
        y: grid.rows[yIndex],
    };
}

function routingEdgeKey(nodeA, nodeB) {
    const first = Math.min(nodeA, nodeB);
    const second = Math.max(nodeA, nodeB);
    return `${first}:${second}`;
}

function parseRoutingEdgeKey(edgeKey) {
    return edgeKey.split(':').map((value) => Number.parseInt(value, 10));
}

function bitCount(mask) {
    let value = mask >>> 0;
    let count = 0;
    while (value) {
        value &= value - 1;
        count += 1;
    }
    return count;
}

function chooseCombinations(values, length) {
    if (length === 0) {
        return [[]];
    }
    if (length > values.length) {
        return [];
    }

    const result = [];

    function visit(startIndex, prefix) {
        if (prefix.length === length) {
            result.push(prefix.slice());
            return;
        }

        for (let index = startIndex; index <= values.length - (length - prefix.length); index++) {
            prefix.push(values[index]);
            visit(index + 1, prefix);
            prefix.pop();
        }
    }

    visit(0, []);
    return result;
}

function firstStageTrunkSlots(targetX, slotCount) {
    const count = slotCount || 6;
    const startX = CLC_SVG.COL_RAIL_X + 28;
    const endX = targetX - 36;
    if (count <= 1) {
        return [snapCoord((startX + endX) / 2)];
    }

    const step = (endX - startX) / (count - 1);
    return Array.from({ length: count }, (_, index) => snapCoord(startX + (index * step)));
}

function segmentOverlapLength(left, right) {
    const leftHorizontal = Math.abs(left[0][1] - left[1][1]) < 1;
    const rightHorizontal = Math.abs(right[0][1] - right[1][1]) < 1;

    if (leftHorizontal !== rightHorizontal) {
        return 0;
    }

    if (leftHorizontal) {
        if (Math.abs(left[0][1] - right[0][1]) >= 1) {
            return 0;
        }
        const leftMin = Math.min(left[0][0], left[1][0]);
        const leftMax = Math.max(left[0][0], left[1][0]);
        const rightMin = Math.min(right[0][0], right[1][0]);
        const rightMax = Math.max(right[0][0], right[1][0]);
        return Math.max(0, Math.min(leftMax, rightMax) - Math.max(leftMin, rightMin));
    }

    if (Math.abs(left[0][0] - right[0][0]) >= 1) {
        return 0;
    }
    const leftMin = Math.min(left[0][1], left[1][1]);
    const leftMax = Math.max(left[0][1], left[1][1]);
    const rightMin = Math.min(right[0][1], right[1][1]);
    const rightMax = Math.max(right[0][1], right[1][1]);
    return Math.max(0, Math.min(leftMax, rightMax) - Math.max(leftMin, rightMin));
}

function segmentIntersectionKind(left, right) {
    const leftHorizontal = Math.abs(left[0][1] - left[1][1]) < 1;
    const rightHorizontal = Math.abs(right[0][1] - right[1][1]) < 1;

    if (leftHorizontal === rightHorizontal) {
        return segmentOverlapLength(left, right) > 0 ? 'overlap' : null;
    }

    const horizontal = leftHorizontal ? left : right;
    const vertical = leftHorizontal ? right : left;
    const hx1 = Math.min(horizontal[0][0], horizontal[1][0]);
    const hx2 = Math.max(horizontal[0][0], horizontal[1][0]);
    const vy1 = Math.min(vertical[0][1], vertical[1][1]);
    const vy2 = Math.max(vertical[0][1], vertical[1][1]);
    const x = vertical[0][0];
    const y = horizontal[0][1];

    if (x < hx1 - 0.5 || x > hx2 + 0.5 || y < vy1 - 0.5 || y > vy2 + 0.5) {
        return null;
    }

    const horizontalEndpoint = (
        (Math.abs(x - horizontal[0][0]) < 1 && Math.abs(y - horizontal[0][1]) < 1)
        || (Math.abs(x - horizontal[1][0]) < 1 && Math.abs(y - horizontal[1][1]) < 1)
    );
    const verticalEndpoint = (
        (Math.abs(x - vertical[0][0]) < 1 && Math.abs(y - vertical[0][1]) < 1)
        || (Math.abs(x - vertical[1][0]) < 1 && Math.abs(y - vertical[1][1]) < 1)
    );

    if (horizontalEndpoint || verticalEndpoint) {
        return 'touch';
    }
    return 'cross';
}

function mergeAxisSegments(segments, horizontal) {
    const grouped = new Map();

    for (const segment of segments) {
        const axisValue = horizontal ? segment[0][1] : segment[0][0];
        const start = horizontal
            ? Math.min(segment[0][0], segment[1][0])
            : Math.min(segment[0][1], segment[1][1]);
        const end = horizontal
            ? Math.max(segment[0][0], segment[1][0])
            : Math.max(segment[0][1], segment[1][1]);

        const ranges = grouped.get(axisValue) || [];
        ranges.push([start, end]);
        grouped.set(axisValue, ranges);
    }

    const merged = [];
    for (const [axisValue, ranges] of grouped.entries()) {
        const ordered = ranges.slice().sort((left, right) => left[0] - right[0] || left[1] - right[1]);
        let current = null;

        for (const range of ordered) {
            if (!current) {
                current = range.slice();
                continue;
            }

            if (range[0] <= current[1] + 0.5) {
                current[1] = Math.max(current[1], range[1]);
                continue;
            }

            merged.push(horizontal
                ? [[current[0], axisValue], [current[1], axisValue]]
                : [[axisValue, current[0]], [axisValue, current[1]]]);
            current = range.slice();
        }

        if (current) {
            merged.push(horizontal
                ? [[current[0], axisValue], [current[1], axisValue]]
                : [[axisValue, current[0]], [axisValue, current[1]]]);
        }
    }

    return merged;
}

function mergeOrthogonalSegments(segments) {
    const horizontal = [];
    const vertical = [];

    for (const segment of segments) {
        if (Math.abs(segment[0][1] - segment[1][1]) < 1) {
            horizontal.push(segment);
        } else {
            vertical.push(segment);
        }
    }

    return [
        ...mergeAxisSegments(horizontal, true),
        ...mergeAxisSegments(vertical, false),
    ];
}

function buildTrunkFirstStageRoute(startX, laneY, targetX, targets, trunkX) {
    const sinks = (targets || [])
        .map((target) => ({ ...target }))
        .sort((left, right) => left.pinY - right.pinY);

    if (!sinks.length) {
        return { segments: [], junctions: [] };
    }

    const rawSegments = [];
    const nodeYs = [laneY, ...sinks.map((sink) => sink.pinY)];
    const minY = Math.min(...nodeYs);
    const maxY = Math.max(...nodeYs);

    rawSegments.push([[startX, laneY], [trunkX, laneY]]);
    if (Math.abs(maxY - minY) >= 1) {
        rawSegments.push([[trunkX, minY], [trunkX, maxY]]);
    }

    for (const sink of sinks) {
        rawSegments.push([[trunkX, sink.pinY], [targetX, sink.pinY]]);
    }

    const segments = mergeOrthogonalSegments(rawSegments);
    const junctions = [];
    const uniqueNodeYs = uniqueSortedNumbers(nodeYs);

    for (const nodeY of uniqueNodeYs) {
        const hasAbove = uniqueNodeYs.some((value) => value < nodeY - 0.5);
        const hasBelow = uniqueNodeYs.some((value) => value > nodeY + 0.5);
        const isSourceNode = Math.abs(nodeY - laneY) < 1;
        const sinkCount = sinks.filter((sink) => Math.abs(sink.pinY - nodeY) < 1).length;
        const degree = (hasAbove ? 1 : 0)
            + (hasBelow ? 1 : 0)
            + sinkCount
            + (isSourceNode ? 1 : 0);

        if (degree > 2) {
            junctions.push([trunkX, nodeY]);
        }
    }

    return {
        segments,
        junctions: dedupePoints(junctions),
    };
}

function routeStageMetrics(route) {
    let horizontal = 0;
    let vertical = 0;
    for (const segment of route.segments) {
        if (Math.abs(segment[0][1] - segment[1][1]) < 1) {
            horizontal += Math.abs(segment[1][0] - segment[0][0]);
        } else {
            vertical += Math.abs(segment[1][1] - segment[0][1]);
        }
    }
    return { horizontal, vertical };
}

function scoreFirstStageRoutes(routes, laneYs, assignment) {
    let crossings = 0;
    let touches = 0;
    let totalVertical = 0;
    let totalHorizontal = 0;

    for (const route of routes) {
        const metrics = routeStageMetrics(route);
        totalHorizontal += metrics.horizontal;
        totalVertical += metrics.vertical;
    }

    for (let leftIndex = 0; leftIndex < routes.length; leftIndex++) {
        for (let rightIndex = leftIndex + 1; rightIndex < routes.length; rightIndex++) {
            for (const leftSegment of routes[leftIndex].segments) {
                for (const rightSegment of routes[rightIndex].segments) {
                    const kind = segmentIntersectionKind(leftSegment, rightSegment);
                    if (kind === 'overlap') {
                        return Number.POSITIVE_INFINITY;
                    }
                    if (kind === 'touch') {
                        touches += 1;
                    } else if (kind === 'cross') {
                        crossings += 1;
                    }
                }
            }
        }
    }

    const displacement = assignment
        .map((trunkX, sourceIndex) => ({ trunkX, sourceIndex }))
        .filter((entry) => Number.isFinite(entry.trunkX))
        .sort((left, right) => left.trunkX - right.trunkX)
        .reduce((sum, entry, slotIndex) => sum + Math.abs(entry.sourceIndex - slotIndex), 0);

    return (touches * 120)
        + (crossings * 28)
        + (totalVertical * 0.18)
        + (totalHorizontal * 0.01)
        + (displacement * 0.5);
}

function getNodeUsage(state, nodeId) {
    let usage = state.nodeUsage.get(nodeId);
    if (!usage) {
        usage = { h: 0, v: 0 };
        state.nodeUsage.set(nodeId, usage);
    }
    return usage;
}

function markNodeUsage(state, nodeId, orientation, sourceIndex) {
    const usage = getNodeUsage(state, nodeId);
    usage[orientation] |= (1 << sourceIndex);
}

function createFirstStageState(grid, laneYs) {
    const originNodes = laneYs.map((laneY) => (
        routingNodeId(grid, 0, grid.rowIndexByValue.get(laneY))
    ));

    return {
        edgeOwners: new Map(),
        nodeUsage: new Map(),
        netEdges: Array.from({ length: 4 }, () => new Set()),
        netNodes: originNodes.map((originNode) => new Set([originNode])),
        originNodes,
    };
}

function routeDirection(fromCoords, toCoords) {
    return fromCoords.y === toCoords.y ? 'h' : 'v';
}

function nodeCrossPenalty(state, nodeId, sourceIndex, orientation) {
    const usage = state.nodeUsage.get(nodeId);
    if (!usage) {
        return 0;
    }
    const oppositeOrientation = orientation === 'h' ? 'v' : 'h';
    const foreignMask = usage[oppositeOrientation] & ~(1 << sourceIndex);
    return bitCount(foreignMask) * FIRST_STAGE_ROUTE.crossingPenalty;
}

function horizontalLanePenalty(y, sourceIndex, laneYs) {
    const overlapsForeignLane = laneYs.some((laneY, index) => (
        index !== sourceIndex && Math.abs(laneY - y) < 1
    ));
    return overlapsForeignLane ? FIRST_STAGE_ROUTE.foreignLanePenalty : 0;
}

function heapPush(heap, item) {
    heap.push(item);
    let index = heap.length - 1;
    while (index > 0) {
        const parentIndex = Math.floor((index - 1) / 2);
        if (heap[parentIndex].f <= item.f) {
            break;
        }
        heap[index] = heap[parentIndex];
        index = parentIndex;
    }
    heap[index] = item;
}

function heapPop(heap) {
    if (!heap.length) {
        return null;
    }
    const root = heap[0];
    const last = heap.pop();
    if (!heap.length) {
        return root;
    }

    let index = 0;
    while (true) {
        const left = (index * 2) + 1;
        const right = left + 1;
        if (left >= heap.length) {
            break;
        }

        const smallerChild = right < heap.length && heap[right].f < heap[left].f
            ? right
            : left;
        if (heap[smallerChild].f >= last.f) {
            break;
        }
        heap[index] = heap[smallerChild];
        index = smallerChild;
    }
    heap[index] = last;
    return root;
}

function reconstructRoutingPath(stateInfo, previousStateKey, stateKey) {
    const path = [];
    let currentKey = stateKey;
    while (currentKey) {
        const info = stateInfo.get(currentKey);
        path.push(info.nodeId);
        currentKey = previousStateKey.get(currentKey) || null;
    }
    path.reverse();
    return path;
}

function neighborNodeIds(grid, nodeId) {
    const coords = routingNodeCoords(grid, nodeId);
    const neighbors = [];

    if (coords.xIndex > 0) {
        neighbors.push(routingNodeId(grid, coords.xIndex - 1, coords.yIndex));
    }
    if (coords.xIndex + 1 < grid.width) {
        neighbors.push(routingNodeId(grid, coords.xIndex + 1, coords.yIndex));
    }
    if (coords.yIndex > 0) {
        neighbors.push(routingNodeId(grid, coords.xIndex, coords.yIndex - 1));
    }
    if (coords.yIndex + 1 < grid.rows.length) {
        neighbors.push(routingNodeId(grid, coords.xIndex, coords.yIndex + 1));
    }

    return neighbors;
}

function estimateRouteCost(grid, nodeId, targetNodeId) {
    const from = routingNodeCoords(grid, nodeId);
    const to = routingNodeCoords(grid, targetNodeId);
    return Math.abs(to.x - from.x) + (Math.abs(to.y - from.y) * FIRST_STAGE_ROUTE.verticalWeight);
}

function verticalColumnPenalty(grid, x) {
    const leftEdge = grid.columns[0];
    const rightEdge = grid.columns[grid.columns.length - 1];
    const preferredColumns = firstStageTrunkSlots(rightEdge);
    const distanceToPreferred = preferredColumns.reduce((best, column) => (
        Math.min(best, Math.abs(column - x))
    ), Number.POSITIVE_INFINITY);

    let penalty = distanceToPreferred * 0.25;
    if (Math.abs(x - leftEdge) < 1) {
        penalty += 90;
    }
    if (Math.abs(x - rightEdge) < 1) {
        penalty += 70;
    }
    return penalty;
}

function routeFirstStageConnection(grid, state, laneYs, sourceIndex, targetNodeId) {
    const treeNodes = state.netNodes[sourceIndex];
    const bestCost = new Map();
    const previousStateKey = new Map();
    const stateInfo = new Map();
    const heap = [];

    for (const nodeId of treeNodes) {
        const stateKey = `${nodeId}:start`;
        const initialState = {
            key: stateKey,
            nodeId,
            direction: 'start',
            g: 0,
            f: estimateRouteCost(grid, nodeId, targetNodeId),
        };
        bestCost.set(stateKey, 0);
        stateInfo.set(stateKey, initialState);
        heapPush(heap, initialState);
    }

    while (heap.length) {
        const current = heapPop(heap);
        if (!current) {
            break;
        }
        if (current.g > bestCost.get(current.key) + 1e-9) {
            continue;
        }
        if (current.nodeId === targetNodeId) {
            return {
                nodes: reconstructRoutingPath(stateInfo, previousStateKey, current.key),
                cost: current.g,
            };
        }

        const currentCoords = routingNodeCoords(grid, current.nodeId);
        const neighbors = neighborNodeIds(grid, current.nodeId);
        for (const neighborNodeId of neighbors) {
            const nextCoords = routingNodeCoords(grid, neighborNodeId);
            const direction = routeDirection(currentCoords, nextCoords);
            const edgeKey = routingEdgeKey(current.nodeId, neighborNodeId);
            const edgeOwner = state.edgeOwners.get(edgeKey);

            if (edgeOwner !== undefined && edgeOwner !== sourceIndex) {
                continue;
            }

            const distance = Math.abs(nextCoords.x - currentCoords.x)
                + Math.abs(nextCoords.y - currentCoords.y);
            let stepCost = direction === 'v'
                ? distance * FIRST_STAGE_ROUTE.verticalWeight
                : distance;

            if (current.direction !== 'start' && current.direction !== direction) {
                stepCost += FIRST_STAGE_ROUTE.bendPenalty;
            }
            if (edgeOwner === sourceIndex) {
                stepCost += FIRST_STAGE_ROUTE.reuseWeight;
            }
            if (direction === 'h') {
                stepCost += horizontalLanePenalty(nextCoords.y, sourceIndex, laneYs);
            } else {
                stepCost += verticalColumnPenalty(grid, currentCoords.x);
            }
            stepCost += nodeCrossPenalty(state, neighborNodeId, sourceIndex, direction);

            const tentativeCost = current.g + stepCost;
            const nextKey = `${neighborNodeId}:${direction}`;
            if (tentativeCost >= (bestCost.get(nextKey) ?? Number.POSITIVE_INFINITY)) {
                continue;
            }

            const nextState = {
                key: nextKey,
                nodeId: neighborNodeId,
                direction,
                g: tentativeCost,
                f: tentativeCost + estimateRouteCost(grid, neighborNodeId, targetNodeId),
            };
            bestCost.set(nextKey, tentativeCost);
            previousStateKey.set(nextKey, current.key);
            stateInfo.set(nextKey, nextState);
            heapPush(heap, nextState);
        }
    }

    return null;
}

function commitFirstStagePath(grid, state, sourceIndex, nodePath) {
    for (let index = 1; index < nodePath.length; index++) {
        const startNodeId = nodePath[index - 1];
        const endNodeId = nodePath[index];
        const startCoords = routingNodeCoords(grid, startNodeId);
        const endCoords = routingNodeCoords(grid, endNodeId);
        const direction = routeDirection(startCoords, endCoords);
        const edgeKey = routingEdgeKey(startNodeId, endNodeId);

        state.edgeOwners.set(edgeKey, sourceIndex);
        state.netEdges[sourceIndex].add(edgeKey);
        state.netNodes[sourceIndex].add(startNodeId);
        state.netNodes[sourceIndex].add(endNodeId);
        markNodeUsage(state, startNodeId, direction, sourceIndex);
        markNodeUsage(state, endNodeId, direction, sourceIndex);
    }
}

function connectionDifficulty(target, sourceIndex, laneYs) {
    const foreignLane = laneYs.some((laneY, index) => (
        index !== sourceIndex && Math.abs(laneY - target.pinY) < 1
    )) ? 100 : 0;
    return foreignLane + Math.abs(target.pinY - laneYs[sourceIndex]);
}

function sortSourceTargets(targets, sourceIndex, laneYs) {
    return targets.slice().sort((left, right) => (
        connectionDifficulty(right, sourceIndex, laneYs)
        - connectionDifficulty(left, sourceIndex, laneYs)
    ) || (left.pinY - right.pinY));
}

function permute(values) {
    if (values.length <= 1) {
        return [values.slice()];
    }

    const result = [];
    const working = values.slice();

    function visit(n) {
        if (n <= 1) {
            result.push(working.slice());
            return;
        }

        visit(n - 1);
        for (let index = 0; index < n - 1; index++) {
            const swapIndex = (n % 2 === 0) ? index : 0;
            [working[swapIndex], working[n - 1]] = [working[n - 1], working[swapIndex]];
            visit(n - 1);
        }
    }

    visit(working.length);
    return result;
}

function mergeRoutingEdges(grid, edgeSet) {
    const horizontalByY = new Map();
    const verticalByX = new Map();

    for (const edgeKey of edgeSet) {
        const [nodeA, nodeB] = parseRoutingEdgeKey(edgeKey);
        const start = routingNodeCoords(grid, nodeA);
        const end = routingNodeCoords(grid, nodeB);

        if (start.y === end.y) {
            const y = start.y;
            const range = [Math.min(start.x, end.x), Math.max(start.x, end.x)];
            const entries = horizontalByY.get(y) || [];
            entries.push(range);
            horizontalByY.set(y, entries);
        } else {
            const x = start.x;
            const range = [Math.min(start.y, end.y), Math.max(start.y, end.y)];
            const entries = verticalByX.get(x) || [];
            entries.push(range);
            verticalByX.set(x, entries);
        }
    }

    const mergedSegments = [];

    function mergeRanges(groupedRanges, pointBuilder) {
        for (const [axis, ranges] of groupedRanges.entries()) {
            const ordered = ranges
                .slice()
                .sort((left, right) => left[0] - right[0] || left[1] - right[1]);
            let current = null;

            for (const range of ordered) {
                if (!current) {
                    current = range.slice();
                    continue;
                }

                if (range[0] <= current[1] + 0.5) {
                    current[1] = Math.max(current[1], range[1]);
                    continue;
                }

                mergedSegments.push(pointBuilder(axis, current[0], current[1]));
                current = range.slice();
            }

            if (current) {
                mergedSegments.push(pointBuilder(axis, current[0], current[1]));
            }
        }
    }

    mergeRanges(horizontalByY, (y, startX, endX) => [[startX, y], [endX, y]]);
    mergeRanges(verticalByX, (x, startY, endY) => [[x, startY], [x, endY]]);

    return mergedSegments;
}

function buildNetJunctions(grid, edgeSet) {
    const degrees = new Map();

    for (const edgeKey of edgeSet) {
        const [nodeA, nodeB] = parseRoutingEdgeKey(edgeKey);
        degrees.set(nodeA, (degrees.get(nodeA) || 0) + 1);
        degrees.set(nodeB, (degrees.get(nodeB) || 0) + 1);
    }

    const junctions = [];
    for (const [nodeId, degree] of degrees.entries()) {
        if (degree <= 2) {
            continue;
        }
        const coords = routingNodeCoords(grid, nodeId);
        junctions.push([coords.x, coords.y]);
    }
    return junctions;
}

function countRoutedNetOverlaps(routed) {
    let overlaps = 0;

    for (let leftIndex = 0; leftIndex < routed.routes.length; leftIndex++) {
        for (let rightIndex = leftIndex + 1; rightIndex < routed.routes.length; rightIndex++) {
            for (const leftSegment of routed.routes[leftIndex].segments) {
                for (const rightSegment of routed.routes[rightIndex].segments) {
                    if (segmentIntersectionKind(leftSegment, rightSegment) === 'overlap') {
                        overlaps += 1;
                    }
                }
            }
        }
    }

    return overlaps;
}

function routeFirstStageNetsOnGrid(sourceConnections, laneYs, targetX) {
    const activeSources = sourceConnections
        .map((targets, sourceIndex) => (targets.length ? sourceIndex : null))
        .filter((value) => value !== null);
    const grid = buildFirstStageGrid(sourceConnections, laneYs, targetX);
    const orders = activeSources.length ? permute(activeSources) : [[]];
    let best = null;

    for (const order of orders) {
        const state = createFirstStageState(grid, laneYs);
        let totalCost = 0;
        let failed = false;

        for (const sourceIndex of order) {
            const sortedTargets = sortSourceTargets(sourceConnections[sourceIndex], sourceIndex, laneYs);
            for (const target of sortedTargets) {
                const targetRowIndex = grid.rowIndexByValue.get(target.pinY);
                const targetColumnIndex = grid.columnIndexByValue.get(targetX);
                const targetNodeId = routingNodeId(grid, targetColumnIndex, targetRowIndex);
                const route = routeFirstStageConnection(
                    grid,
                    state,
                    laneYs,
                    sourceIndex,
                    targetNodeId
                );

                if (!route) {
                    failed = true;
                    break;
                }

                commitFirstStagePath(grid, state, sourceIndex, route.nodes);
                totalCost += route.cost;
            }
            if (failed) {
                break;
            }
        }

        if (failed) {
            continue;
        }

        const candidate = {
            grid,
            netEdges: state.netEdges,
            routes: state.netEdges.map((edgeSet) => ({
                segments: mergeRoutingEdges(grid, edgeSet),
                junctions: buildNetJunctions(grid, edgeSet),
            })),
            cost: totalCost,
            order: order.slice(),
        };

        if (!best || candidate.cost < best.cost - 1e-9) {
            best = candidate;
            continue;
        }

        if (Math.abs(candidate.cost - best.cost) < 1e-9) {
            const displacement = order.reduce((sum, sourceIndex, slot) => sum + Math.abs(sourceIndex - slot), 0);
            const bestDisplacement = best.order.reduce((sum, sourceIndex, slot) => sum + Math.abs(sourceIndex - slot), 0);
            if (displacement < bestDisplacement) {
                best = candidate;
            }
        }
    }

    if (best) {
        return best;
    }

    return {
        grid,
        netEdges: Array.from({ length: 4 }, () => new Set()),
        routes: Array.from({ length: 4 }, () => ({ segments: [], junctions: [] })),
        cost: Number.POSITIVE_INFINITY,
        order: [],
    };
}

function routeFirstStageNets(sourceConnections, laneYs, targetX) {
    const activeSources = sourceConnections
        .map((targets, sourceIndex) => (targets.length ? sourceIndex : null))
        .filter((value) => value !== null);
    const slotSets = chooseCombinations(firstStageTrunkSlots(targetX), activeSources.length);
    const orders = activeSources.length ? permute(activeSources) : [[]];
    let best = null;

    for (const slotSet of slotSets) {
        for (const order of orders) {
            const assignment = new Array(4).fill(Number.NaN);
            order.forEach((sourceIndex, slotIndex) => {
                assignment[sourceIndex] = slotSet[slotIndex];
            });

            const routes = sourceConnections.map((targets, sourceIndex) => (
                targets.length
                    ? buildTrunkFirstStageRoute(
                        CLC_SVG.COL_RAIL_X,
                        laneYs[sourceIndex],
                        targetX,
                        targets,
                        assignment[sourceIndex]
                    )
                    : { segments: [], junctions: [] }
            ));

            const cost = scoreFirstStageRoutes(routes, laneYs, assignment);
            if (!Number.isFinite(cost)) {
                continue;
            }

            const candidate = {
                grid: null,
                netEdges: Array.from({ length: 4 }, () => new Set()),
                routes,
                cost,
                order: order.slice(),
                assignment,
            };

            if (!best || candidate.cost < best.cost - 1e-9) {
                best = candidate;
                continue;
            }

            if (Math.abs(candidate.cost - best.cost) < 1e-9) {
                const displacement = order.reduce((sum, sourceIndex, slot) => sum + Math.abs(sourceIndex - slot), 0);
                const bestDisplacement = best.order.reduce((sum, sourceIndex, slot) => sum + Math.abs(sourceIndex - slot), 0);
                if (displacement < bestDisplacement) {
                    best = candidate;
                }
            }
        }
    }

    if (!best) {
        return routeFirstStageNetsOnGrid(sourceConnections, laneYs, targetX);
    }

    return best;
}

function dedupePoints(points) {
    const seen = new Set();
    const unique = [];
    for (const point of points) {
        const key = `${point[0]},${point[1]}`;
        if (seen.has(key)) {
            continue;
        }
        seen.add(key);
        unique.push(point);
    }
    return unique;
}

function renderRouteSegments(layers, segments, cls) {
    for (const points of segments) {
        const [start, ...rest] = points;
        if (rest.length === 1) {
            const end = rest[0];
            layers.wires.appendChild(svgLine(start[0], start[1], end[0], end[1], cls));
            continue;
        }

        layers.wires.appendChild(svgPolyline(points, cls));
    }
}

function renderSourceFanouts(layers, traces, sourceConnections) {
    const routed = routeFirstStageNets(sourceConnections, CLC_SVG.LANES, GATE_RENDER.symX);
    for (let d = 0; d < 4; d++) {
        const route = routed.routes[d];
        const cls = `${sourceTraceClass(d, traces['rail-' + d])} clc-wire`;
        renderRouteSegments(layers, route.segments, cls);
        for (const [x, y] of route.junctions) {
            layers.junctions.appendChild(svgDot(x, y, sourceTraceClass(d, traces['rail-' + d])));
        }
    }
}

function chooseAlignedPinY(preferredY, minY, maxY, blockedYs, minSpacing) {
    const clampY = (value) => Math.max(minY, Math.min(maxY, value));
    const normalizedPreferred = clampY(preferredY);
    const blocked = (blockedYs || [])
        .map(value => clampY(value))
        .sort((left, right) => left - right);
    const spacing = Math.max(0, minSpacing || 0);
    const collides = (candidate) => blocked.some(value => Math.abs(value - candidate) < spacing);

    if (!collides(normalizedPreferred)) {
        return normalizedPreferred;
    }

    const candidates = [];
    for (const value of blocked) {
        candidates.push(clampY(value - spacing));
        candidates.push(clampY(value + spacing));
    }
    candidates.push(minY);
    candidates.push(maxY);

    const viable = dedupePoints(candidates.map(value => [value, 0]))
        .map(([value]) => value)
        .filter(candidate => !collides(candidate))
        .sort((left, right) => Math.abs(left - preferredY) - Math.abs(right - preferredY));

    return viable[0] ?? normalizedPreferred;
}

function pathMetrics(points) {
    let horizontal = 0;
    let vertical = 0;

    for (let i = 1; i < points.length; i++) {
        const [x1, y1] = points[i - 1];
        const [x2, y2] = points[i];
        horizontal += Math.abs(x2 - x1);
        vertical += Math.abs(y2 - y1);
    }

    return {
        horizontal,
        vertical,
        bends: Math.max(points.length - 2, 0),
    };
}

function scoreOrthogonalPath(points, weights) {
    const metrics = pathMetrics(points);
    const w = {
        horizontal: 1,
        vertical: 1,
        bends: 1,
        ...(weights || {}),
    };

    return (metrics.horizontal * w.horizontal)
        + (metrics.vertical * w.vertical)
        + (metrics.bends * w.bends);
}

function buildMode2inDffLayoutCandidate(S, andY) {
    const andX = S.COL_CORE_X + 10;
    const stubX = andX - 10;
    const andOut = andX + GATE_OUT_DX;
    const ffX = andOut + 16;
    const boxY = S.LANES[0] - 20;
    // Keep mode 5 on the same vertical envelope as the other sequential
    // blocks so the centered D/Q pins do not read artificially low.
    const boxH = S.LANES[3] - S.LANES[0] + 40;
    const clkY = S.LANES[0];
    const rY = S.LANES[2];
    const dY = chooseAlignedPinY(andY, boxY + 20, boxY + boxH - 20, [clkY, rY], 22);
    const inputSegments = buildGateStubSegments(stubX, gateInputX('and', andX), [
        {
            gate: 1,
            laneY: S.LANES[1],
            pinY: andY - GATE_PIN_DY,
            active: true,
            cls: '',
        },
        {
            gate: 3,
            laneY: S.LANES[3],
            pinY: andY + GATE_PIN_DY,
            active: true,
            cls: '',
        },
    ]).map(segment => segment.points);
    const dPath = Math.abs(andY - dY) < 1
        ? [[andOut, andY], [ffX, dY]]
        : [[andOut, andY], [andOut + 6, andY], [andOut + 6, dY], [ffX, dY]];
    const upperVertical = Math.abs(S.LANES[1] - (andY - GATE_PIN_DY));
    const lowerVertical = Math.abs(S.LANES[3] - (andY + GATE_PIN_DY));
    const semanticTargetY = S.CENTER_Y;
    const score = inputSegments.reduce((total, segment) => (
        total + scoreOrthogonalPath(segment, { horizontal: 1, vertical: 5, bends: 16 })
    ), 0)
        + scoreOrthogonalPath(dPath, { horizontal: 1, vertical: 7, bends: 20 })
        + (Math.max(upperVertical, lowerVertical) * 2)
        + (Math.abs(upperVertical - lowerVertical) * 2)
        + (Math.abs(andY - semanticTargetY) * 8)
        + (Math.abs(dY - semanticTargetY) * 10);

    return {
        andX,
        andY,
        stubX,
        andOut,
        ffX,
        dY,
        score,
    };
}

function chooseMode2inDffLayout(S) {
    const minAndY = Math.ceil(S.LANES[1] + GATE_PIN_DY);
    const maxAndY = Math.floor(S.LANES[3] - GATE_PIN_DY);
    let best = null;

    for (let andY = minAndY; andY <= maxAndY; andY++) {
        const candidate = buildMode2inDffLayoutCandidate(S, andY);
        if (!best || candidate.score < best.score - 1e-9) {
            best = candidate;
            continue;
        }

        if (Math.abs(candidate.score - best.score) < 1e-9) {
            const candidateAlignment = Math.abs(candidate.andY - candidate.dY);
            const bestAlignment = Math.abs(best.andY - best.dY);
            if (candidateAlignment < bestAlignment) {
                best = candidate;
            }
        }
    }

    return best;
}

function buildGateInputSlots(gateY, inputCount) {
    const maxSpan = snapCoord(GATE_RENDER.bodyH - 6);
    const inputStep = inputCount > 1 ? maxSpan / (inputCount - 1) : 0;
    const inputStartY = inputCount > 1 ? gateY - (maxSpan / 2) : gateY;
    return Array.from({ length: inputCount }, (_, index) => snapCoord(inputStartY + (index * inputStep)));
}

function gateInputSlotCost(input, slotY, slotIndex, inputIndex) {
    const sourceLaneY = CLC_SVG.LANES[input.ds];
    let cost = Math.abs(sourceLaneY - slotY) * 2;

    // A foreign branch that lands exactly on another source's horizontal lane
    // creates the kind of different-net overlap seen in the screenshots.
    const overlapsForeignLane = CLC_SVG.LANES.some((laneY, ds) => (
        ds !== input.ds && Math.abs(laneY - slotY) < 1
    ));
    if (overlapsForeignLane) {
        cost += 1000;
    }

    // Keep layouts stable when costs tie so the schematic does not jump around.
    cost += Math.abs(slotIndex - inputIndex) * 0.01;
    return cost;
}

function collectEnabledGateInputs(gate, gateY) {
    const enabledInputs = gate.inputs.filter(input => input.enabled);
    const inputCount = enabledInputs.length;
    if (!inputCount) {
        return [];
    }

    const slotYs = buildGateInputSlots(gateY, inputCount);
    const memo = new Map();

    function solve(inputIndex, usedMask) {
        if (inputIndex >= inputCount) {
            return { cost: 0, slots: [] };
        }

        const memoKey = `${inputIndex}:${usedMask}`;
        if (memo.has(memoKey)) {
            return memo.get(memoKey);
        }

        let best = null;
        for (let slotIndex = 0; slotIndex < inputCount; slotIndex++) {
            if (usedMask & (1 << slotIndex)) {
                continue;
            }

            const slotY = slotYs[slotIndex];
            const localCost = gateInputSlotCost(enabledInputs[inputIndex], slotY, slotIndex, inputIndex);
            const next = solve(inputIndex + 1, usedMask | (1 << slotIndex));
            const candidate = {
                cost: localCost + next.cost,
                slots: [slotY, ...next.slots],
            };

            if (!best || candidate.cost < best.cost) {
                best = candidate;
            }
        }

        memo.set(memoKey, best);
        return best;
    }

    const assignment = solve(0, 0);
    return enabledInputs.map((input, index) => ({
        ...input,
        pinY: assignment.slots[index],
    }));
}

/**
 * Column C→D: Data rails with gate connections and gate blocks.
 *
 * Only enabled gate inputs produce visible rail-to-gate wires.
 * Disabled connections are omitted entirely to avoid visual clutter.
 * All wires are strictly horizontal or vertical (Manhattan routing).
 */
function renderGates(layers, sem, traces) {
    const S = CLC_SVG;
    const sourceConnections = [[], [], [], []];
    const gateSymX = GATE_RENDER.symX;

    for (let g = 0; g < 4; g++) {
        const gateY = S.LANES[g];
        const gateCls = traceClass(traces['gate-' + g]);

        const gateOutX = placeGate(layers, 'and', gateSymX, gateY, gateCls + ' clc-sym');

        // Gate label
        const label = svgText(gateLabelX('and', gateSymX), gateY - GATE_RENDER.bodyH / 2 - 14,
            'G' + (g + 1), gateCls + ' clc-label-gate', 'middle');
        layers.labels.appendChild(label);

        // Keep every enabled literal as a distinct routed input so T/N pairs do
        // not collapse into a single visual connection.
        const enabledInputs = collectEnabledGateInputs(sem.gates[g], gateY);
        enabledInputs.forEach(input => {
            sourceConnections[input.ds].push({
                gate: g,
                pinY: input.pinY,
            });
        });

        // Gate output: optional inversion bubble at gate tip, then wire
        if (sem.gates[g].inverted) {
            // Negation bubble flush at gate output tip
            const bubbleX = gateOutX + S.BUBBLE_R;
            layers.symbols.appendChild(
                svgCircle(bubbleX, gateY, S.BUBBLE_R, gateCls + ' clc-sym clc-bubble'));
            // Wire from bubble right edge to gate-out column
            layers.wires.appendChild(
                svgLine(gateOutX + S.BUBBLE_R * 2, gateY, S.COL_GATE_OUT, gateY,
                    gateCls + ' clc-wire'));
        } else {
            // Wire from gate tip to gate-out column
            layers.wires.appendChild(
                svgLine(gateOutX, gateY, S.COL_GATE_OUT, gateY, gateCls + ' clc-wire'));
        }
    }

    renderSourceFanouts(layers, traces, sourceConnections);
}

/** Columns F+G: Output conditioning chain and final output pin */
function renderOutputChain(layers, sem, traces, coreOutput) {
    const S = CLC_SVG;
    const y = coreOutput?.y ?? S.CENTER_Y;
    const outActive = traces['output'];
    const cls = traceClass(outActive);

    let wireStartX = coreOutput?.x ?? S.COL_CORE_OUT;

    // LCPOL inversion bubble
    if (sem.output.lcpol) {
        // Keep the inversion bubble visually attached to the mode-core output
        // instead of letting it float at the conditioning column boundary.
        const lcpolLead = 4;
        const bx = snapCoord(wireStartX + lcpolLead + S.BUBBLE_R);
        // Wire from core output to bubble
        layers.wires.appendChild(svgLine(wireStartX, y, bx - S.BUBBLE_R, y, cls + ' clc-wire'));
        layers.symbols.appendChild(svgCircle(bx, y, S.BUBBLE_R, cls + ' clc-sym clc-bubble'));
        wireStartX = bx + S.BUBBLE_R;

        const polLabel = svgText(bx, y - S.BUBBLE_R - 6, 'LCPOL',
            cls + ' clc-label-cond', 'middle');
        layers.labels.appendChild(polLabel);
    }

    // LCOE indicator
    const lcoeX = S.COL_COND_X + 50;
    if (sem.output.lcoe) {
        layers.wires.appendChild(svgLine(wireStartX, y, lcoeX, y, cls + ' clc-wire'));
    } else {
        // Show break in wire for disabled output
        layers.wires.appendChild(svgLine(wireStartX, y, lcoeX - 8, y, cls + ' clc-wire'));
        layers.wires.appendChild(svgLine(lcoeX + 8, y, lcoeX + 16, y,
            traceClass(false) + ' clc-wire'));
        const oeLabel = svgText(lcoeX, y + 14, 'LCOE off',
            'clc-trace-dim clc-label-cond', 'middle');
        layers.labels.appendChild(oeLabel);
    }

    // Interrupt taps
    if (sem.output.intp || sem.output.intn) {
        const tapX = lcoeX + 30;
        if (sem.output.intp) {
            const tap = svgPolyline([[tapX, y], [tapX, y - 28], [tapX + 16, y - 28]],
                cls + ' clc-wire');
            layers.wires.appendChild(tap);
            layers.labels.appendChild(svgText(tapX + 18, y - 28, 'INTP',
                cls + ' clc-label-cond', 'start'));
            layers.junctions.appendChild(svgDot(tapX, y, cls));
        }
        if (sem.output.intn) {
            const tap = svgPolyline([[tapX, y], [tapX, y + 28], [tapX + 16, y + 28]],
                cls + ' clc-wire');
            layers.wires.appendChild(tap);
            layers.labels.appendChild(svgText(tapX + 18, y + 28, 'INTN',
                cls + ' clc-label-cond', 'start'));
            if (!sem.output.intp) {
                layers.junctions.appendChild(svgDot(tapX, y, cls));
            }
        }
    }

    // Wire to output pin
    const pinWireStart = sem.output.lcoe ? lcoeX : lcoeX + 16;
    layers.wires.appendChild(svgLine(pinWireStart, y, S.COL_PIN_X - 12, y, cls + ' clc-wire'));

    // Output pin circle
    layers.symbols.appendChild(svgCircle(S.COL_PIN_X - 6, y, 5, cls + ' clc-sym'));

    // Output label
    const modIdx = (typeof clcActiveModule !== 'undefined') ? clcActiveModule : 1;
    layers.labels.appendChild(svgText(S.COL_PIN_X + 4, y, 'CLC' + modIdx + 'OUT',
        cls + ' clc-label-out', 'start'));

    // LCEN overlay when disabled
    if (!sem.output.lcen) {
        const overlay = svgRect(2, 2, S.W - 4, S.H - 4, 'clc-disabled-overlay', 6, 'none');
        overlay.style.fill = 'var(--bg)';
        overlay.style.fillOpacity = '0.5';
        overlay.style.stroke = 'none';
        layers.symbols.appendChild(overlay);

        const disLabel = svgText(S.W / 2, S.H / 2 + 50, 'Module disabled (LCEN = 0)',
            'clc-trace-dim clc-label-disabled', 'middle');
        layers.labels.appendChild(disLabel);
    }
}

// =============================================================================
// Mode core templates (Column E)
// =============================================================================

/**
 * Route gate outputs horizontally into mode core inputs.
 *
 * Core input pin Y positions are aligned to the gate lane Y values,
 * so every gate→core wire is a single horizontal segment. Unused gate
 * outputs are omitted entirely so partial configurations do not leave
 * dangling mode-core wires behind. The only vertical routing happens
 * inside the mode core templates.
 */
function routeGatesToCore(layers, traces, pins) {
    const S = CLC_SVG;
    for (const pin of pins) {
        const g = pin.gate;
        if (!traces['gate-out-' + g]) {
            continue;
        }
        const gateY = S.LANES[g];
        const cls = traceClass(traces['gate-out-' + g]);
        const startX = S.COL_GATE_OUT;

        if (Math.abs(gateY - pin.y) < 2) {
            // Horizontal — the normal case with lane-aligned pins
            layers.wires.appendChild(svgLine(startX, gateY, pin.x, gateY, cls + ' clc-wire'));
        } else {
            // Manhattan fallback for modes that can't fully align
            const chanX = S.CHAN_X[g];
            layers.wires.appendChild(svgPolyline(
                [[startX, gateY], [chanX, gateY], [chanX, pin.y], [pin.x, pin.y]],
                cls + ' clc-wire'));
        }
    }
}

// --- Shared gate rendering ---

const GATE_GEOMETRY = Object.freeze({
    and: {
        // ANSI/IEEE Std 91: D-shape — flat left, semicircular right
        outDx: 33,
        inDx: 0,
        labelDy: 22,
        paths: [
            { d: 'M0,-15 L18,-15 A15,15 0 0,1 18,15 L0,15 Z', fill: 'var(--bg)' },
        ],
    },
    or: {
        // ANSI/IEEE Std 91: concave back curve, convex top/bottom meeting at tip
        outDx: 36,
        inDx: 4,
        labelDy: 22,
        paths: [
            {
                d: 'M0,-15 C14,-15 28,-8 36,0 C28,8 14,15 0,15 C5,7 5,-7 0,-15 Z',
                fill: 'var(--bg)',
            },
        ],
    },
    xor: {
        // ANSI/IEEE Std 91: OR body shifted right + exclusive-input back curve
        outDx: 41,
        inDx: 5,
        labelDy: 22,
        paths: [
            {
                d: 'M5,-15 C19,-15 33,-8 41,0 C33,8 19,15 5,15 C10,7 10,-7 5,-15 Z',
                fill: 'var(--bg)',
            },
            { d: 'M0,-15 C5,-7 5,7 0,15', fill: 'none' },
        ],
    },
});
const GATE_OUT_DX = snapCoord(GATE_GEOMETRY.and.outDx * GATE_SC);

function appendGatePath(group, cls, d, fill) {
    group.appendChild(svgEl('path', {
        d,
        fill: fill || 'none',
        class: cls,
        'stroke-width': CLC_SVG.SYM_W,
        'stroke-linejoin': 'round',
        'stroke-linecap': 'round',
        'vector-effect': 'non-scaling-stroke',
    }));
}

/**
 * Place a fixed-size logic gate symbol at (x, y).
 * Returns the output X coordinate.  All gates use the same scale.
 */
function placeGate(layers, type, x, y, cls) {
    const geometry = GATE_GEOMETRY[type] || GATE_GEOMETRY.and;
    const g = svgEl('g', { transform: `translate(${x},${y}) scale(${GATE_SC})` });
    for (const path of geometry.paths) {
        appendGatePath(g, cls, path.d, path.fill);
    }
    layers.symbols.appendChild(g);
    return x + (geometry.outDx * GATE_SC);
}

function gateInputX(type, gateX) {
    const geometry = GATE_GEOMETRY[type] || GATE_GEOMETRY.and;
    return snapCoord(gateX + (geometry.inDx * GATE_SC));
}

function gateOutputX(type, gateX) {
    const geometry = GATE_GEOMETRY[type] || GATE_GEOMETRY.and;
    return snapCoord(gateX + (geometry.outDx * GATE_SC));
}

function gateLabelX(type, gateX) {
    return snapCoord((gateInputX(type, gateX) + gateOutputX(type, gateX)) / 2);
}

function buildBinaryGateInputRoutes(type, gateX, gateY, upperSource, lowerSource) {
    const pinX = gateInputX(type, gateX);
    const channelX = snapCoord(pinX - (10 * GATE_SC));
    const topPinY = snapCoord(gateY - GATE_PIN_DY);
    const botPinY = snapCoord(gateY + GATE_PIN_DY);
    const routes = [
        { source: upperSource, pinY: topPinY },
        { source: lowerSource, pinY: botPinY },
    ];

    return routes.map(({ source, pinY }) => {
        if (Math.abs(source.y - pinY) < 1) {
            return {
                cls: source.cls,
                points: [[source.x, source.y], [pinX, pinY]],
            };
        }

        return {
            cls: source.cls,
            points: [[source.x, source.y], [channelX, source.y], [channelX, pinY], [pinX, pinY]],
        };
    });
}

function placeModeLabelAboveGate(layers, type, gateX, gateY, text, cls) {
    const geometry = GATE_GEOMETRY[type] || GATE_GEOMETRY.and;
    layers.labels.appendChild(svgText(
        gateLabelX(type, gateX),
        gateY - (geometry.labelDy * GATE_SC),
        text,
        cls + ' clc-label-mode',
        'middle'
    ));
}

/** Scaled input pin offset (±8 at gate center = top/bottom inputs). */
const GATE_PIN_DY = 10;

function activeCorePins(traces, pins) {
    return pins.filter(pin => traces['gate-out-' + pin.gate]);
}

/**
 * Build the Manhattan stub segments that connect gate-lane outputs into a
 * mode-core sub-gate. Inactive inputs are skipped so grouped modes never
 * render branches that terminate in empty space.
 */
function buildGateStubSegments(stubX, pinX, inputs) {
    const segments = [];

    for (const input of inputs) {
        if (!input || !input.active) {
            continue;
        }

        const laneY = input.laneY;
        const pinY = input.pinY;
        const bendX = Number.isFinite(input.channelX) ? input.channelX : stubX;
        let points;

        if (Math.abs(laneY - pinY) < 1) {
            points = [[stubX, laneY], [pinX, pinY]];
        } else if (Math.abs(bendX - stubX) < 1) {
            points = [[stubX, laneY], [stubX, pinY], [pinX, pinY]];
        } else {
            points = [[stubX, laneY], [bendX, laneY], [bendX, pinY], [pinX, pinY]];
        }

        segments.push({
            cls: input.cls,
            points,
        });
    }

    return segments;
}

function renderGateStubSegments(layers, segments) {
    for (const segment of segments) {
        const [start, ...rest] = segment.points;
        if (rest.length === 1) {
            const end = rest[0];
            layers.wires.appendChild(
                svgLine(start[0], start[1], end[0], end[1], segment.cls)
            );
            continue;
        }

        layers.wires.appendChild(svgPolyline(segment.points, segment.cls));
    }
}

/** Mode 0: AND-OR — G1·G2 + G3·G4 */
function renderModeAndOr(layers, sem, traces) {
    const S = CLC_SVG;
    const cx = S.COL_CORE_X;
    const cy = S.CENTER_Y;

    const cls1 = traceClass(traces['gate-out-0'] || traces['gate-out-1']);
    const cls2 = traceClass(traces['gate-out-2'] || traces['gate-out-3']);

    // Two AND gates at group centers
    const gateX = cx + 20;
    const and1Out = placeGate(layers, 'and', gateX, S.GROUP1_Y, cls1 + ' clc-sym');
    const and2Out = placeGate(layers, 'and', gateX, S.GROUP2_Y, cls2 + ' clc-sym');

    // Stubs from lane Y to gate inputs
    const stubX = cx + 8;
    renderGateStubSegments(layers, buildGateStubSegments(stubX, gateInputX('and', gateX), [
        {
            gate: 0,
            laneY: S.LANES[0],
            pinY: S.GROUP1_Y - GATE_PIN_DY,
            active: traces['gate-out-0'],
            cls: traceClass(traces['gate-out-0']) + ' clc-wire',
        },
        {
            gate: 1,
            laneY: S.LANES[1],
            pinY: S.GROUP1_Y + GATE_PIN_DY,
            active: traces['gate-out-1'],
            cls: traceClass(traces['gate-out-1']) + ' clc-wire',
        },
    ]));
    renderGateStubSegments(layers, buildGateStubSegments(stubX, gateInputX('and', gateX), [
        {
            gate: 2,
            laneY: S.LANES[2],
            pinY: S.GROUP2_Y - GATE_PIN_DY,
            active: traces['gate-out-2'],
            cls: traceClass(traces['gate-out-2']) + ' clc-wire',
        },
        {
            gate: 3,
            laneY: S.LANES[3],
            pinY: S.GROUP2_Y + GATE_PIN_DY,
            active: traces['gate-out-3'],
            cls: traceClass(traces['gate-out-3']) + ' clc-wire',
        },
    ]));

    // OR gate at center
    const orX = and1Out + 24;
    const orActive = traces['core-out'];
    const orCls = traceClass(orActive);
    const orOut = placeGate(layers, 'or', orX, cy, orCls + ' clc-sym');

    renderGateStubSegments(layers, buildBinaryGateInputRoutes('or', orX, cy,
        { x: and1Out, y: S.GROUP1_Y, cls: cls1 + ' clc-wire' },
        { x: and2Out, y: S.GROUP2_Y, cls: cls2 + ' clc-wire' }));

    placeModeLabelAboveGate(layers, 'or', orX, cy, 'AND-OR', orCls);

    // Gate outputs → stub entry points (horizontal at lane Y)
    routeGatesToCore(layers, traces, [
        { gate: 0, x: stubX, y: S.LANES[0] },
        { gate: 1, x: stubX, y: S.LANES[1] },
        { gate: 2, x: stubX, y: S.LANES[2] },
        { gate: 3, x: stubX, y: S.LANES[3] },
    ]);

    return { x: orOut, y: cy };
}

/** Mode 1: OR-XOR — (G1+G2) XOR (G3+G4) */
function renderModeOrXor(layers, sem, traces) {
    const S = CLC_SVG;
    const cx = S.COL_CORE_X;
    const cy = S.CENTER_Y;

    const cls1 = traceClass(traces['gate-out-0'] || traces['gate-out-1']);
    const cls2 = traceClass(traces['gate-out-2'] || traces['gate-out-3']);

    const gateX = cx + 20;
    const or1Out = placeGate(layers, 'or', gateX, S.GROUP1_Y, cls1 + ' clc-sym');
    const or2Out = placeGate(layers, 'or', gateX, S.GROUP2_Y, cls2 + ' clc-sym');

    const stubX = cx + 8;
    renderGateStubSegments(layers, buildGateStubSegments(stubX, gateInputX('or', gateX), [
        {
            gate: 0,
            laneY: S.LANES[0],
            pinY: S.GROUP1_Y - GATE_PIN_DY,
            active: traces['gate-out-0'],
            cls: traceClass(traces['gate-out-0']) + ' clc-wire',
        },
        {
            gate: 1,
            laneY: S.LANES[1],
            pinY: S.GROUP1_Y + GATE_PIN_DY,
            active: traces['gate-out-1'],
            cls: traceClass(traces['gate-out-1']) + ' clc-wire',
        },
    ]));
    renderGateStubSegments(layers, buildGateStubSegments(stubX, gateInputX('or', gateX), [
        {
            gate: 2,
            laneY: S.LANES[2],
            pinY: S.GROUP2_Y - GATE_PIN_DY,
            active: traces['gate-out-2'],
            cls: traceClass(traces['gate-out-2']) + ' clc-wire',
        },
        {
            gate: 3,
            laneY: S.LANES[3],
            pinY: S.GROUP2_Y + GATE_PIN_DY,
            active: traces['gate-out-3'],
            cls: traceClass(traces['gate-out-3']) + ' clc-wire',
        },
    ]));

    // XOR gate at center
    const xorX = or1Out + 24;
    const xorActive = traces['core-out'];
    const xorCls = traceClass(xorActive);
    const xorOut = placeGate(layers, 'xor', xorX, cy, xorCls + ' clc-sym');

    renderGateStubSegments(layers, buildBinaryGateInputRoutes('xor', xorX, cy,
        { x: or1Out, y: S.GROUP1_Y, cls: cls1 + ' clc-wire' },
        { x: or2Out, y: S.GROUP2_Y, cls: cls2 + ' clc-wire' }));

    placeModeLabelAboveGate(layers, 'xor', xorX, cy, 'OR-XOR', xorCls);

    routeGatesToCore(layers, traces, [
        { gate: 0, x: stubX, y: S.LANES[0] },
        { gate: 1, x: stubX, y: S.LANES[1] },
        { gate: 2, x: stubX, y: S.LANES[2] },
        { gate: 3, x: stubX, y: S.LANES[3] },
    ]);

    return { x: xorOut, y: cy };
}

/** Mode 2: 4-AND — G1 · G2 · G3 · G4 */
function renderMode4And(layers, sem, traces) {
    const S = CLC_SVG;
    const cx = S.COL_CORE_X;
    const cy = S.CENTER_Y;
    const active = traces['core-out'];
    const cls = traceClass(active);

    // IEC-style multi-input function block keeps all four gate outputs aligned
    // to their natural lane Y positions, which avoids unnecessary crossings.
    const blockX = cx + 56;
    const blockW = 62;
    const pinStubX = blockX - 12;
    const boxY = S.LANES[0] - 18;
    const boxH = S.LANES[3] - S.LANES[0] + 36;

    layers.symbols.appendChild(svgRect(blockX, boxY, blockW, boxH, cls + ' clc-sym', 6));
    layers.labels.appendChild(svgText(blockX + blockW / 2, boxY - 10, '4-AND',
        cls + ' clc-label-mode', 'middle'));
    layers.labels.appendChild(svgText(blockX + blockW / 2, cy, 'AND',
        cls + ' clc-label-pin', 'middle'));

    for (let i = 0; i < 4; i++) {
        const inputCls = traceClass(traces['gate-out-' + i]);
        layers.wires.appendChild(svgLine(pinStubX, S.LANES[i], blockX, S.LANES[i],
            inputCls + ' clc-wire'));
    }

    routeGatesToCore(layers, traces, [
        { gate: 0, x: pinStubX, y: S.LANES[0] },
        { gate: 1, x: pinStubX, y: S.LANES[1] },
        { gate: 2, x: pinStubX, y: S.LANES[2] },
        { gate: 3, x: pinStubX, y: S.LANES[3] },
    ]);

    return { x: blockX + blockW, y: cy };
}

/** Mode 3: SR Latch — S=(G1+G2), R=(G3+G4) */
function renderModeSrLatch(layers, sem, traces) {
    const S = CLC_SVG;
    const cx = S.COL_CORE_X;
    const cy = S.CENTER_Y;

    const cls1 = traceClass(traces['gate-out-0'] || traces['gate-out-1']);
    const cls2 = traceClass(traces['gate-out-2'] || traces['gate-out-3']);

    // Two OR gates at group centers
    const gateX = cx + 20;
    const or1Out = placeGate(layers, 'or', gateX, S.GROUP1_Y, cls1 + ' clc-sym');
    const or2Out = placeGate(layers, 'or', gateX, S.GROUP2_Y, cls2 + ' clc-sym');

    const stubX = cx + 8;
    renderGateStubSegments(layers, buildGateStubSegments(stubX, gateInputX('or', gateX), [
        {
            gate: 0,
            laneY: S.LANES[0],
            pinY: S.GROUP1_Y - GATE_PIN_DY,
            active: traces['gate-out-0'],
            cls: traceClass(traces['gate-out-0']) + ' clc-wire',
        },
        {
            gate: 1,
            laneY: S.LANES[1],
            pinY: S.GROUP1_Y + GATE_PIN_DY,
            active: traces['gate-out-1'],
            cls: traceClass(traces['gate-out-1']) + ' clc-wire',
        },
    ]));
    renderGateStubSegments(layers, buildGateStubSegments(stubX, gateInputX('or', gateX), [
        {
            gate: 2,
            laneY: S.LANES[2],
            pinY: S.GROUP2_Y - GATE_PIN_DY,
            active: traces['gate-out-2'],
            cls: traceClass(traces['gate-out-2']) + ' clc-wire',
        },
        {
            gate: 3,
            laneY: S.LANES[3],
            pinY: S.GROUP2_Y + GATE_PIN_DY,
            active: traces['gate-out-3'],
            cls: traceClass(traces['gate-out-3']) + ' clc-wire',
        },
    ]));

    // SR Latch box — S at GROUP1_Y, R at GROUP2_Y (horizontal from OR outputs)
    const boxX = or1Out + 16;
    const boxW = 64;
    const sY = S.GROUP1_Y;
    const rY = S.GROUP2_Y;
    const boxY = sY - 24;
    const boxH = (rY - sY) + 48;
    const boxActive = traces['core-out'];
    const boxCls = traceClass(boxActive);

    layers.symbols.appendChild(svgRect(boxX, boxY, boxW, boxH, boxCls + ' clc-sym', 4));
    layers.labels.appendChild(svgText(boxX + 8, sY, 'S', boxCls + ' clc-label-pin', 'start'));
    layers.labels.appendChild(svgText(boxX + 8, rY, 'R', boxCls + ' clc-label-pin', 'start'));
    layers.labels.appendChild(svgText(boxX + boxW - 8, cy, 'Q',
        boxCls + ' clc-label-pin', 'end'));
    layers.labels.appendChild(svgText(boxX + boxW / 2, boxY - 8, 'SR Latch',
        boxCls + ' clc-label-mode', 'middle'));

    // OR → latch (horizontal — same Y)
    layers.wires.appendChild(svgLine(or1Out, sY, boxX, sY, cls1 + ' clc-wire'));
    layers.wires.appendChild(svgLine(or2Out, rY, boxX, rY, cls2 + ' clc-wire'));

    routeGatesToCore(layers, traces, [
        { gate: 0, x: stubX, y: S.LANES[0] },
        { gate: 1, x: stubX, y: S.LANES[1] },
        { gate: 2, x: stubX, y: S.LANES[2] },
        { gate: 3, x: stubX, y: S.LANES[3] },
    ]);

    return { x: boxX + boxW, y: cy };
}

/**
 * Shared renderer for sequential modes (4-7): arbitrary box with pins
 * at gate lane Y positions for horizontal gate→core wires.
 */
function renderSequentialCore(layers, sem, traces, title, pins) {
    const S = CLC_SVG;
    const cx = S.COL_CORE_X + 30;
    const cy = S.CENTER_Y;
    const boxY = S.LANES[0] - 20;
    const boxH = S.LANES[3] - S.LANES[0] + 40;
    const boxW = S.FF_W;
    const active = traces['core-out'];
    const cls = traceClass(active);

    layers.symbols.appendChild(svgRect(cx, boxY, boxW, boxH, cls + ' clc-sym', 4));
    layers.labels.appendChild(svgText(cx + boxW / 2, boxY - 8, title,
        cls + ' clc-label-mode', 'middle'));

    // Q output at vertical center
    layers.labels.appendChild(svgText(cx + boxW - 8, cy, 'Q', cls + ' clc-label-pin', 'end'));

    const corePins = [];
    for (const pin of pins) {
        const pinY = pin.y;
        layers.labels.appendChild(svgText(cx + 10, pinY, pin.label,
            cls + ' clc-label-pin', 'start'));

        if (pin.clk) {
            const tri = svgEl('g', { transform: `translate(${cx},${pinY})` });
            tri.innerHTML = `<polygon points="0,-5 8,0 0,5" fill="none"
                stroke-width="1" vector-effect="non-scaling-stroke" class="${cls} clc-sym" />`;
            layers.symbols.appendChild(tri);
        }

        layers.wires.appendChild(svgLine(cx - 10, pinY, cx, pinY, cls + ' clc-wire'));
        corePins.push({ gate: pin.gate, x: cx - 10, y: pinY });
    }

    routeGatesToCore(layers, traces, corePins);
    return { x: cx + boxW, y: cy };
}

/** Mode 4: D-FF + S/R — D=G2, CLK=G1, S=G4, R=G3 */
function renderModeDff(layers, sem, traces) {
    const S = CLC_SVG;
    return renderSequentialCore(layers, sem, traces, 'D-FF', [
        { label: 'CLK', gate: 0, y: S.LANES[0], clk: true },
        { label: 'D',   gate: 1, y: S.LANES[1] },
        { label: 'R',   gate: 2, y: S.LANES[2] },
        { label: 'S',   gate: 3, y: S.LANES[3] },
    ]);
}

/** Mode 5: 2-in D-FF + R — D=G2·G4, CLK=G1, R=G3 */
function renderMode2inDff(layers, sem, traces) {
    const S = CLC_SVG;
    const cy = S.CENTER_Y;
    const andActive = traces['gate-out-1'] || traces['gate-out-3'];
    const andCls = traceClass(andActive);
    const layout = chooseMode2inDffLayout(S);

    // Place the pre-AND stage using a scored candidate search that penalizes
    // vertical motion and extra bends on the derived D path.
    const andY = layout.andY;
    const andX = layout.andX;
    const andOut = placeGate(layers, 'and', andX, andY, andCls + ' clc-sym');

    // Stubs from lane Y to AND inputs
    const stubX = layout.stubX;
    renderGateStubSegments(layers, buildGateStubSegments(stubX, gateInputX('and', andX), [
        {
            gate: 1,
            laneY: S.LANES[1],
            pinY: andY - GATE_PIN_DY,
            active: traces['gate-out-1'],
            cls: traceClass(traces['gate-out-1']) + ' clc-wire',
        },
        {
            gate: 3,
            laneY: S.LANES[3],
            pinY: andY + GATE_PIN_DY,
            active: traces['gate-out-3'],
            cls: traceClass(traces['gate-out-3']) + ' clc-wire',
        },
    ]));

    // G2 and G4 → stub (horizontal at lane Y)
    routeGatesToCore(layers, traces, [
        { gate: 1, x: stubX, y: S.LANES[1] },
        { gate: 3, x: stubX, y: S.LANES[3] },
    ]);

    // FF box
    const ffX = layout.ffX;
    const boxY = S.LANES[0] - 20;
    const boxH = S.LANES[3] - S.LANES[0] + 40;
    const active = traces['core-out'];
    const cls = traceClass(active);

    layers.symbols.appendChild(svgRect(ffX, boxY, S.FF_W, boxH, cls + ' clc-sym', 4));
    layers.labels.appendChild(svgText(ffX + S.FF_W / 2, boxY - 8, 'D-FF',
        cls + ' clc-label-mode', 'middle'));

    // CLK at lane 0 (horizontal)
    const clkY = S.LANES[0];
    layers.labels.appendChild(svgText(ffX + 10, clkY, 'CLK', cls + ' clc-label-pin', 'start'));
    const tri = svgEl('g', { transform: `translate(${ffX},${clkY})` });
    tri.innerHTML = `<polygon points="0,-5 8,0 0,5" fill="none"
        stroke-width="1" vector-effect="non-scaling-stroke" class="${cls} clc-sym" />`;
    layers.symbols.appendChild(tri);
    layers.wires.appendChild(svgLine(ffX - 10, clkY, ffX, clkY, cls + ' clc-wire'));
    routeGatesToCore(layers, traces, [
        { gate: 0, x: ffX - 10, y: clkY },
    ]);

    // D pin — chosen by the same scored layout pass as the pre-AND gate.
    const dY = layout.dY;
    layers.labels.appendChild(svgText(ffX + 10, dY, 'D', cls + ' clc-label-pin', 'start'));
    if (Math.abs(andY - dY) < 1) {
        layers.wires.appendChild(svgLine(andOut, andY, ffX, dY, andCls + ' clc-wire'));
    } else {
        layers.wires.appendChild(svgPolyline(
            [[andOut, andY], [andOut + 6, andY], [andOut + 6, dY], [ffX, dY]],
            andCls + ' clc-wire'));
    }

    // R at lane 2 (horizontal)
    const rY = S.LANES[2];
    layers.labels.appendChild(svgText(ffX + 10, rY, 'R', cls + ' clc-label-pin', 'start'));
    layers.wires.appendChild(svgLine(ffX - 10, rY, ffX, rY, cls + ' clc-wire'));
    routeGatesToCore(layers, traces, [
        { gate: 2, x: ffX - 10, y: rY },
    ]);

    // Q output — keep the special-case D-FF aligned with the same core
    // centerline used by the other sequential-mode templates.
    const qY = cy;
    layers.labels.appendChild(svgText(ffX + S.FF_W - 8, qY, 'Q',
        cls + ' clc-label-pin', 'end'));
    return { x: ffX + S.FF_W, y: qY };
}

/** Mode 6: JK-FF + R — J=G2, CLK=G1, K=G4, R=G3 */
function renderModeJkff(layers, sem, traces) {
    const S = CLC_SVG;
    return renderSequentialCore(layers, sem, traces, 'JK-FF', [
        { label: 'CLK', gate: 0, y: S.LANES[0], clk: true },
        { label: 'J',   gate: 1, y: S.LANES[1] },
        { label: 'R',   gate: 2, y: S.LANES[2] },
        { label: 'K',   gate: 3, y: S.LANES[3] },
    ]);
}

/** Mode 7: Latch + S/R — D=G2, LE=G1, S=G4, R=G3 */
function renderModeLatch(layers, sem, traces) {
    const S = CLC_SVG;
    return renderSequentialCore(layers, sem, traces, 'D-Latch', [
        { label: 'LE',  gate: 0, y: S.LANES[0] },
        { label: 'D',   gate: 1, y: S.LANES[1] },
        { label: 'R',   gate: 2, y: S.LANES[2] },
        { label: 'S',   gate: 3, y: S.LANES[3] },
    ]);
}

/** Dispatch to the correct mode template renderer. */
const MODE_RENDERERS = [
    renderModeAndOr,    // 0
    renderModeOrXor,    // 1
    renderMode4And,     // 2
    renderModeSrLatch,  // 3
    renderModeDff,      // 4
    renderMode2inDff,   // 5
    renderModeJkff,     // 6
    renderModeLatch,    // 7
];

// =============================================================================
// Stage 3: Top-level render
// =============================================================================

function renderClcSchematic() {
    const container = document.getElementById('clc-schematic-container');
    if (!container) return;

    const mod = clcConfig[clcActiveModule];
    if (!mod) {
        container.innerHTML = '';
        return;
    }

    const sem = buildClcSemanticModel(mod);
    const traces = resolveActiveTraces(sem);

    // Build fresh SVG each render (the DOM is small enough)
    const S = CLC_SVG;
    const svg = svgEl('svg', {
        viewBox: `0 0 ${S.W} ${S.H}`,
        preserveAspectRatio: 'xMidYMid meet',
        class: 'clc-schematic',
    });

    const layers = {
        wires:     svgEl('g', { class: 'layer-wires' }),
        junctions: svgEl('g', { class: 'layer-junctions' }),
        symbols:   svgEl('g', { class: 'layer-symbols' }),
        labels:    svgEl('g', { class: 'layer-labels' }),
    };

    // Render columns left to right
    renderSources(layers, sem, traces);
    renderGates(layers, sem, traces);

    // Mode core (column E)
    const modeRenderer = MODE_RENDERERS[sem.mode.value] || MODE_RENDERERS[0];
    const coreOutput = modeRenderer(layers, sem, traces);

    // Output conditioning + final pin (columns F+G)
    renderOutputChain(layers, sem, traces, coreOutput);

    // Append layers in draw order
    svg.appendChild(layers.wires);
    svg.appendChild(layers.junctions);
    svg.appendChild(layers.symbols);
    svg.appendChild(layers.labels);

    container.innerHTML = '';
    container.appendChild(svg);
}

if (typeof module !== 'undefined' && module.exports) {
    module.exports = {
        CLC_SVG,
        GATE_PIN_DY,
        activeCorePins,
        buildFirstStageGrid,
        buildBinaryGateInputRoutes,
        buildGateStubSegments,
        buildMode2inDffLayoutCandidate,
        countRoutedNetOverlaps,
        collectEnabledGateInputs,
        chooseAlignedPinY,
        chooseMode2inDffLayout,
        gateInputX,
        gateLabelX,
        gateOutputX,
        pathMetrics,
        placeModeLabelAboveGate,
        routeFirstStageConnection,
        routeFirstStageNets,
        scoreOrthogonalPath,
        sourceTraceClass,
        traceClass,
    };
}
