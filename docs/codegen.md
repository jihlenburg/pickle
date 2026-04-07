# Code Generation

pickle generates a two-file output set. By default the pair is:

- `mcu_init.h`
- `mcu_init.c`

The basename is configurable through `settings.toml` under
`[codegen].output_basename`, and the frontend code/export/compile-check flows
all consume the configured names instead of assuming `mcu_init`.

The generator is driven by `generate_code` from the Tauri command layer. Its
phase orchestration lives in `src-tauri/src/codegen/generate.rs`, with shared
generator data types in `generate_types.rs`, dedicated PPS emission in
`generate_pps.rs`, dedicated port/analog emission in `generate_ports.rs`,
single-translation-unit merge helpers in `generate_single_file.rs`,
generic formatting/text helpers in `generate_support.rs`, and CLC-specific
packing/emission in `generate_clc.rs`.

## Output Shape

### Header Output

The header contains:

- include guard
- `#include <xc.h>`
- optional signal-name macros (`*_PORT`, `*_LAT`, `*_TRIS`)
- prototypes for any generated helper functions
- `void system_init(void);`

### Source Output

The source contains:

- file header comment and `#include "<configured basename>.h"`
- optional oscillator `#pragma config` lines and `configure_oscillator()`
- optional fuse `#pragma config` sections
- optional `configure_pps()`
- always-present `configure_ports()`
- optional `configure_analog()` for on-chip op-amp enable
- optional `configure_clc()`
- `system_init()` in hardware-safe call order

Compile-check flows that still need a single translation unit reuse
`src-tauri/src/codegen/generate_single_file.rs` to inline the generated signal
alias macros into the emitted C file.

## Generation Order

The generator emits code in this order:

1. oscillator pragmas and oscillator init, when configured
2. fuse pragmas, when selected
3. PPS setup
4. port setup
5. optional analog block
6. optional CLC block
7. `system_init()`

That order is deliberate: clock configuration comes first, PPS is bound before pins are driven, and peripheral-enablement steps happen after base port state is established.

## PPS Generation

`configure_pps()` is emitted by `src-tauri/src/codegen/generate_pps.rs`.

Remappable inputs and outputs are split into separate blocks and wrapped in the required RPCON unlock/lock sequence.

```c
void configure_pps(void)
{
    __builtin_write_RPCON(0x0000U);

    RPINR18bits.U1RXR = 36U;  /* U1RX <- RP36/RB4 */
    RPOR1bits.RP37R   = 1U;   /* RP37/RB5 -> U1TX */

    __builtin_write_RPCON(0x0800U);
}
```

Important details:

- input mappings probe both `U1RXR` and `U1RX` naming variants because device packs are inconsistent
- output mappings require both the RP register field and the peripheral `ppsval`
- comments are column-aligned so generated blocks stay readable

## Port Configuration

`configure_ports()` and `configure_analog()` are emitted by `src-tauri/src/codegen/generate_ports.rs`.

`configure_ports()` is always emitted. It currently manages:

- `ANSELx` for analog/digital mode
- `TRISx` for direction
- comments for ICSP/debug reservations

Example:

```c
void configure_ports(void)
{
    /* ICSP/debug pins — directly controlled by the debug module (FICD.ICS) */
    /* RB3 reserved for PGD1 — no ANSEL/TRIS configuration needed */

    ANSELBbits.ANSELB0 = 0U;
    ANSELBbits.ANSELB1 = 1U;

    TRISBbits.TRISB0 = 0U;  /* U1TX (out) */
    TRISBbits.TRISB1 = 1U;  /* AN6 (in) */
}
```

Notes:

- explicit digital overrides are respected even without a peripheral assignment
- analog-mode detection is driven by generated assignment content
- ICSP/debug functions are not configured directly; they are documented and skipped

## Signal Macros

When the UI assigns a signal name to a pin, the header emits macros for direct register access:

```c
#define MOTOR_ENABLE_PORT  (PORTBbits.RB5)
#define MOTOR_ENABLE_LAT   (LATBbits.LATB5)
#define MOTOR_ENABLE_TRIS  (TRISBbits.TRISB5)
```

Non-identifier characters are normalized to `_` and names are uppercased before emission.

## Oscillator Generation

Oscillator generation is delegated through `oscillator.rs`, which now acts as a
public facade over:

- `src-tauri/src/codegen/oscillator/model.rs` for shared `OscConfig`,
  `PLLResult`, PLL search, and managed fuse-field ownership
- `src-tauri/src/codegen/oscillator/legacy.rs` for pragma-driven CK/PIC24
  oscillator output
- `src-tauri/src/codegen/oscillator/ak.rs` for dsPIC33AK runtime clock output

Supported sources:

| Source | Meaning |
|---|---|
| `frc` | Internal FRC, no PLL |
| `frc_pll` | FRC through PLL |
| `pri` | Primary oscillator, no PLL |
| `pri_pll` | Primary oscillator through PLL |
| `lprc` | Low-power RC |

PLL search spans valid `N1`, `M`, `N2`, and `N3` combinations and chooses the closest valid result that satisfies dsPIC33 constraints.

Important family split:

- dsPIC33CK and related 16-bit families use the existing `FNOSC` / `POSCMD` / `PLLKEN` pragma-driven path
- dsPIC33AK moved clock selection into runtime SFRs (`OSCCFG`, `CLK1CON`, `CLK1DIV`, `PLL1CON`, `PLL1DIV`, and related ready bits)
- pickle now emits the shared dsPIC33AK runtime sequence as well:
  - direct `CLK1CON` switching for `frc`, `lprc`, and `pri`
  - `PLL1CON` / `PLL1DIV` setup plus `CLK1CON` handoff to `PLL1 Fout` for `frc_pll` and `pri_pll`
  - polling of `OSWEN`, `CLKRDY`, and `OSCCTRLbits.PLL1RDY`

When enabled, oscillator output contributes:

- top-of-file `#pragma config` lines on CK-style families, or AK design-intent comments
- `configure_oscillator()` with register writes
- a `system_init()` call to `configure_oscillator()`

When oscillator config is enabled on CK-style parts, it owns the overlapping
clock-related fuse fields (`FNOSC`, `IESO`, `POSCMD`, `XTCFG`, `FCKSM`, and
`PLLKEN` when PLL is used). The generic fuse section intentionally suppresses
those fields so generated output cannot emit duplicate `#pragma config`
definitions.

For dsPIC33AK, those legacy clock fields are also stripped if they appear in an
incoming fuse block because they are no longer the authoritative clock-control
surface on that family.

## Fuse Generation

Fuse generation is delegated to `fuses.rs` and uses the device's parsed `fuse_defs`.

The frontend sends selections as:

```json
{
  "selections": {
    "FICD": {
      "ICS": "PGD1",
      "JTAGEN": "OFF"
    }
  }
}
```

The generator turns those into aligned pragma sections such as:

```c
/* FICD: ICD Configuration */
#pragma config ICS = PGD1       /* ICSP channel */
#pragma config JTAGEN = OFF     /* JTAG disabled */
```

## Analog Helper Generation

If any fixed assignment references an on-chip op-amp (`OA1OUT`, `OA2IN+`, and similar), the generator emits:

```c
void configure_analog(void)
{
    AMPCON1Lbits.AMPEN1 = 1U;  /* Enable Op-Amp 1 */
}
```

On dsPIC33AK the emitted register name changes to the newer per-instance form,
for example `AMP1CON1bits.AMPEN = 1U;`.

This helper only enables the modules. Gain, routing, and other analog behavior remain application-specific.

## CLC Generation

When the UI sends populated `ClcModuleConfig` entries, the generator emits
`configure_clc()`.

See [CLC](clc.md) for the shared frontend/backend module shape and the exact
bit-packing contract used by both the designer register preview and the Rust
generator.

CK-style generated registers:

| Register | Meaning |
|---|---|
| `CLCnSEL` | data-source selectors |
| `CLCnGLSL` / `CLCnGLSH` | gate source-enable matrices |
| `CLCnCONH` | gate polarity bits |
| `CLCnCONL` | mode, output polarity, interrupts, enable |

The module is always written in two phases:

1. write `CLCnCONL = 0x0000U` to disable it during setup
2. write the packed registers
3. write final `CLCnCONL` with or without `LCEN`

For dsPIC33AK, pickle emits the unified 32-bit form instead:

| Register | Meaning |
|---|---|
| `CLCxSEL` | data-source selectors |
| `CLCxGLS` | full 32-bit gate source-enable matrix |
| `CLCxCON` | mode, output polarity, gate polarity, interrupts, enable |

The AK path still follows the same disable-then-configure discipline:

1. write `CLCxCON = 0x00000000U` to disable it during setup
2. write `CLCxSEL`
3. write `CLCxGLS`
4. write final `CLCxCON` with or without `ON`

What is still out of scope is not generic CLC packing anymore, but family-specific
peripheral behavior around it, especially the broader dsPIC33AKxxxMPS high-speed
PWM and power-domain flows.

## Single-File Compile Mode

`generate_c_code()` is a compatibility helper used by compile-check flows that need one translation unit. It:

- generates the normal header and source pair
- replaces the generated local header include with `#include <xc.h>`
- inlines generated signal macros so the Microchip family compiler can compile the result without a separate header file on disk

The compile-check path still derives its temporary source/header filenames from
the generated local header include before this rewrite step, so custom output
basenames stay aligned across generation and compiler validation.

## Validation Coverage

The generator is covered by unit and integration tests for:

- header/source file creation
- PPS unlock/lock order
- `system_init()` ordering
- ICSP exclusion
- oscillator/fuse output
- signal macro generation
- CLC register generation
