# dsPIC33 / PIC24 Notes

This page captures the device-level concepts that matter to pickle's parser and generator.

## Part Numbers

Example:

```text
DSPIC33CK64MP102T-E/M6VAO
│       │  │   │ │ │ │  └── qualification / sales suffix
│       │  │   │ │ │ └──── package code
│       │  │   │ │ └────── temperature grade
│       │  │   │ └──────── tape-and-reel suffix
│       │  │   └────────── package / pin-count variant
│       │  └────────────── sub-family
│       └───────────────── flash size
└───────────────────────── family
```

pickle normalizes lookup keys to the base part number such as `DSPIC33CK64MP102`.

## Device Family Packs

Microchip publishes device metadata in `.atpack` archives. The files pickle actually cares about are:

- `edc/*.PIC` XML files for pin, PPS, and fuse data
- pack index metadata used to discover matching archives

The backend can work from:

- previously extracted EDC files
- cached parsed JSON under `devices/`
- downloaded/extracted `.atpack` data under `dfp_cache/`

## EDC Data Model

The XML describes several things the app relies on:

- physical pads and package pinouts
- remappable input/output definitions
- configuration-register metadata (`DCR`, fields, values)
- port, RP, and analog-channel metadata

Those are collapsed into `DeviceData`, which is the canonical backend model handed to the frontend.

## PPS

Peripheral Pin Select is the dsPIC33/PIC24 remapping mechanism for many digital peripherals.

Basic sequence:

1. unlock PPS with `__builtin_write_RPCON(0x0000U)`
2. write input selections to `RPINRn`
3. write output selections to `RPORn`
4. lock PPS with `__builtin_write_RPCON(0x0800U)`

Examples:

- input: `RPINR18bits.U1RXR = 36U`
- output: `RPOR1bits.RP37R = 1U`

Key points for pickle:

- input and output mappings come from EDC pack metadata
- RP numbering is usually derived from port position but should be treated as parsed data, not a guessed formula
- not every package pin is remappable

## ICSP And JTAG

Programming/debug pins are special and should not be treated like normal GPIO.

### ICSP

Typical debug channel pairs:

| Channel | Clock | Data |
|---|---|---|
| 1 | `PGC1` | `PGD1` |
| 2 | `PGC2` | `PGD2` |
| 3 | `PGC3` | `PGD3` |

`FICD.ICS` selects the active pair. pickle excludes ICSP/debug functions from generated port configuration and emits reservation comments instead.

### JTAG

When `JTAGEN = ON`, JTAG-related pins (`TDI`, `TDO`, `TMS`, `TCK`) should also be treated as reserved. The frontend applies the reservation dynamically so assignment UI reflects the active fuse choice.

## Fuses

Device configuration registers are programmed through Microchip XC-family `#pragma config` lines (`xc16-gcc` for PIC24, `xc-dsc-gcc` for dsPIC33).

Common fields surfaced by pickle include:

| Register | Field | Meaning |
|---|---|---|
| `FICD` | `ICS` | debug channel selection |
| `FICD` | `JTAGEN` | JTAG enable |
| `FWDT` | `FWDTEN` | watchdog policy |
| `FWDT` | `WDTPS` | watchdog prescaler |
| `FOSCSEL` | `FNOSC` | initial oscillator source |
| `FOSC` | `POSCMD` | primary oscillator mode |
| `FBORPOR` | `BOREN` / `BORV` | brown-out behavior |

The backend does not hardcode values when it can avoid it; it prefers the parsed field/value list from the device pack.

## Oscillator System

Relevant sources handled by pickle:

- `frc`
- `frc_pll`
- `pri`
- `pri_pll`
- `lprc`

For PLL-backed modes the code searches across valid divider ranges:

| Parameter | Range |
|---|---|
| `N1` | 1–8 |
| `M` | 16–200 |
| `N2` | 1–7 |
| `N3` | 1–7 |

With dsPIC33CK constraints:

- `Fvco`: 400 MHz to 1.6 GHz
- `FPFD`: at least 8 MHz

The backend chooses the closest valid result to the requested `Fosc`.

## Port Registers

The generator currently reasons about:

| Register | Meaning |
|---|---|
| `TRISx` | direction |
| `PORTx` | readback |
| `LATx` | output latch access in generated aliases |
| `ANSELx` | analog/digital mode |

Important nuance: many dsPIC pins default to analog mode. A digital assignment therefore requires explicitly clearing the matching `ANSELx` bit.

## Pinout Overlays

Datasheets sometimes document package variants or corrections that do not appear cleanly in the EDC pack. pickle supports overlay files under `pinouts/`:

```json
{
  "packages": {
    "QFN-48": {
      "pin_count": 48,
      "source": "overlay",
      "pins": {
        "1": "RB0",
        "2": "RB1"
      }
    }
  }
}
```

These overlays are merged into the device at load time and can be created either manually or from verification results.

## CLC

Configurable Logic Cell modules expose:

- four 3-bit data-source selectors (`DS1` to `DS4`)
- four gates with true/complement source enables
- gate polarity bits
- a logic mode
- an output stage and optional interrupts

Supported mode values in the UI/generator:

| Value | Mode |
|---|---|
| `0` | AND-OR |
| `1` | OR-XOR |
| `2` | 4-input AND |
| `3` | S-R latch |
| `4` | 1-input D flip-flop with S/R |
| `5` | 2-input D flip-flop with R |
| `6` | J-K flip-flop with R |
| `7` | transparent latch with S/R |

### Source mapping priority

CLC source labels are resolved in this order:

1. `clc_sources/<PART>.json`
2. LLM-extracted data saved from verification
3. hardcoded fallback mappings for known modules
4. generic `CLCINn` labels when nothing better exists
