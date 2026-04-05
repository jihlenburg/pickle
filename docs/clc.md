# CLC

This page documents pickle's full Configurable Logic Cell pipeline:

- device-specific source-label discovery
- frontend state, editor, and schematic behavior
- config-file persistence shape
- backend register packing and generated C output
- test coverage for the shared invariants

## Scope

pickle currently treats the CLC feature as one cross-cutting subsystem:

- `frontend/static/app/05-clc-model.js` owns defaults, normalization, mode metadata, and register packing
- `frontend/static/app/05-clc-designer.js` owns the DOM-based editor and register preview
- `frontend/static/app/05-clc-schematic.js` renders the SVG schematic preview
- `src-tauri/src/codegen/generate.rs` owns the backend `ClcModuleConfig` shape and emitted `configure_clc()` code

The frontend and backend intentionally share the same logical field layout so a
saved config can round-trip through the UI, code generation, and compile-check
without special translation glue.

## Device Data And Source Labels

When a device is loaded, the backend includes `clc_input_sources` in the
frontend payload when known.

Source labels are resolved in this precedence order:

1. `clc_sources/<PART>.json`
2. LLM-extracted data persisted from verification
3. built-in fallback mappings for known devices
4. generic `CLCINn` labels

The designer exposes four selector groups, `DS1..DS4`, each with eight source
slots (`0..7`). The selected numeric values are the values written into
`CLCnSEL`.

## Frontend Data Model

CLC state is stored as a 1-indexed map of four modules. The default shape for a
single module is:

```json
{
  "ds": [0, 0, 0, 0],
  "gates": [
    [false, false, false, false, false, false, false, false],
    [false, false, false, false, false, false, false, false],
    [false, false, false, false, false, false, false, false],
    [false, false, false, false, false, false, false, false]
  ],
  "gpol": [false, false, false, false],
  "mode": 0,
  "lcpol": false,
  "lcoe": true,
  "lcen": true,
  "intp": false,
  "intn": false
}
```

Field meanings:

- `ds[0..3]`: `DS1..DS4` MUX values, `0..7`
- `gates[gate][bit]`: per-gate source-enable matrix
- `gpol[0..3]`: gate polarity inversion bits
- `mode`: `MODE<2:0>`
- `lcpol`: output inversion
- `lcoe`: output-to-pin enable
- `lcen`: module enable
- `intp`, `intn`: rising/falling interrupt enables

Gate bit order is fixed and shared across the UI and backend:

```text
[D1T, D1N, D2T, D2N, D3T, D3N, D4T, D4N]
```

Important semantic rule:

- `DnT` and `DnN` are independent literals
- if both are enabled for one gate, pickle treats them as two distinct active
  input paths
- they must not collapse into one logical or visual connection

`05-clc-model.js` normalizes saved data aggressively:

- missing modules are restored to defaults
- missing booleans become `false`
- `ds` values are masked to 3 bits
- `mode` is masked to 3 bits
- `lcoe` and `lcen` default to `true`

Only configured modules are persisted or sent to code generation. Modules that
remain at their normalized defaults are omitted.

## Logic Modes

The editor exposes eight mode cards:

| Value | Name | Meaning |
|---|---|---|
| `0` | AND-OR | `Gate1·Gate2 + Gate3·Gate4` |
| `1` | OR-XOR | `(G1 + G2) XOR (G3 + G4)` |
| `2` | 4-AND | `Gate1 · Gate2 · Gate3 · Gate4` |
| `3` | SR Latch | `S=(G1+G2), R=(G3+G4)` |
| `4` | D-FF + S/R | `D=G2, CLK=G1, S=G4, R=G3` |
| `5` | 2-in D-FF + R | `D=G2·G4, CLK=G1, R=G3` |
| `6` | JK-FF + R | `J=G2, CLK=G1, K=G4, R=G3` |
| `7` | Latch + S/R | `D=G2, LE=G1, S=G4, R=G3` |

These names are used consistently in:

- mode cards in the frontend
- register preview context
- generated `configure_clc()` comments in C output

## Register Preview And Packing

The frontend register preview and backend code generation both derive their
values from the same logical rules.

### `CLCnCONL`

Packed as:

- bits `0..2`: `mode`
- bit `5`: `lcpol`
- bit `7`: `lcoe`
- bit `10`: `intn`
- bit `11`: `intp`
- bit `15`: `lcen` when the module is enabled in the final emitted value

The frontend preview shows the fully packed `CONL` value. The backend also
keeps a disabled variant so codegen can write `0x0000U` first before the final
enable write.

### `CLCnCONH`

- bits `0..3`: `gpol[0..3]`

### `CLCnSEL`

Packed as:

- bits `0..2`: `DS1`
- bits `4..6`: `DS2`
- bits `8..10`: `DS3`
- bits `12..14`: `DS4`

### `CLCnGLSL` / `CLCnGLSH`

The first two gates live in `GLSL`; the second two gates live in `GLSH`.

- `GLSL`: Gate 1 in bits `0..7`, Gate 2 in bits `8..15`
- `GLSH`: Gate 3 in bits `0..7`, Gate 4 in bits `8..15`

Within each gate, bit order is still:

```text
D1T, D1N, D2T, D2N, D3T, D3N, D4T, D4N
```

## Schematic Preview

The schematic preview is not a generic graph-layout engine. It is a
deterministic datasheet-style renderer with a constrained orthogonal router.

Pipeline:

1. `buildClcSemanticModel()` converts module state into sources, gates, mode,
   and output semantics
2. `resolveActiveTraces()` marks which parts of the diagram are active
3. `renderClcSchematic()` builds the final SVG

### Layout Rules

- fixed left-to-right structure: source labels -> DS boxes -> first-stage
  routing -> gates -> mode core -> output chain
- orthogonal wiring only
- grid-snapped routing
- deterministic geometry for the same module state
- ANSI-style gate symbols and flip-flop/latch blocks

### Routing Invariants

The first stage is the hard part: routing the four selected source nets into
the gate inputs.

The renderer now enforces these rules:

- different nets may cross
- different nets must never overlap on the same colinear wire segment
- same-net branch reuse is allowed
- true tee junctions get dots
- simple crossings do not imply connectivity

This is intentional. For schematic readability, crossings are acceptable;
overlaps are not.

The source-side wires are colorized by source index in the current UI to make
routing issues easier to spot. That coloring is diagnostic and readability
oriented; the logical invariant is still enforced by the router itself.

### Mode-Core Rendering

The mode core is rendered from fixed templates, not from a generic netlist
solver:

- grouped combinational modes (`AND-OR`, `OR-XOR`, `4-AND`)
- latch/flip-flop modes
- output-chain decorations for `LCPOL`, `LCOE`, and interrupt taps

This is why the preview can look like a datasheet block diagram without needing
an EDA-grade auto-router.

## Config Files

Saved configs persist CLC state under the top-level `clc` key:

```json
{
  "clc": {
    "1": {
      "ds": [0, 1, 2, 3],
      "gates": [
        [true, false, false, false, false, false, false, false],
        [false, true, false, false, false, false, false, false],
        [false, false, true, false, false, false, false, false],
        [false, false, false, true, false, false, false, false]
      ],
      "gpol": [false, true, false, false],
      "mode": 1,
      "lcpol": false,
      "lcoe": true,
      "lcen": true,
      "intp": false,
      "intn": false
    }
  }
}
```

Notes:

- module keys are stringified numbers in JSON
- only configured modules are persisted
- loading runs through normalization before the UI uses the data

## Backend Code Generation

The backend type is `ClcModuleConfig` in
`src-tauri/src/codegen/generate.rs`. Its field semantics intentionally mirror
the frontend model.

When any configured modules are present, code generation emits:

- `void configure_clc(void);` in the header
- a `configure_clc()` implementation in the source
- a `system_init()` call to `configure_clc()`

Each configured module is emitted in this order:

1. `CLCnCONL = 0x0000U;` to disable the module before reconfiguration
2. `CLCnSEL`
3. `CLCnGLSL`
4. `CLCnGLSH`
5. `CLCnCONH`
6. final `CLCnCONL`

The final `CONL` write:

- includes `LCEN` when `lcen` is true
- omits `LCEN` when the user intentionally keeps the module disabled

Generated comments include:

- the module number
- the resolved mode name
- the selected `DS1..DS4` values
- enable/output/inversion summary for the final `CONL`

## Request / Response Boundaries

Frontend `generate_code` requests send CLC data as a module-number keyed object:

```json
{
  "request": {
    "clc": {
      "1": {
        "ds": [0, 1, 2, 3],
        "gates": [
          [true, false, false, false, false, false, false, false],
          [false, true, false, false, false, false, false, false],
          [false, false, true, false, false, false, false, false],
          [false, false, false, true, false, false, false, false]
        ],
        "gpol": [false, false, false, false],
        "mode": 1,
        "lcpol": false,
        "lcoe": true,
        "lcen": true,
        "intp": false,
        "intn": false
      }
    }
  }
}
```

`load_device` responses may include:

```json
{
  "clc_input_sources": [
    ["CLCINA", "..."],
    ["CLCINB", "..."],
    ["CLCINC", "..."],
    ["CLCIND", "..."]
  ]
}
```

Verification results may also include extracted `clc_input_sources`, which can
then be saved into `clc_sources/<PART>.json`.

## Tests

CLC behavior is covered in multiple places:

- `frontend/tests/clc-model.test.js`
  - defaults
  - normalization
  - register packing
  - configured-module extraction
- `frontend/tests/clc-schematic.test.js`
  - grouped-mode stub behavior
  - source-net coloring
  - first-stage routing invariants
  - no-overlap guarantee
- `src-tauri/src/codegen/generate.rs` unit tests
  - CLC register generation
  - codegen ordering
- `./scripts/validate.sh`
  - runs the full frontend and backend CLC-related suites

## Maintenance Rules

When the CLC feature changes, keep these in sync in the same patch:

- `frontend/static/app/05-clc-model.js`
- `frontend/static/app/05-clc-designer.js`
- `frontend/static/app/05-clc-schematic.js`
- `src-tauri/src/codegen/generate.rs`
- this document

If a change affects public behavior, also update:

- `docs/codegen.md`
- `docs/commands.md`
- `docs/domain.md`
