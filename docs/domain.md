# dsPIC33 Domain Knowledge

Background on Microchip dsPIC33 concepts relevant to pickle.

## Part Number Format

```
DSPIC33CK64MP102T-E/M6VAO
│       │  │   │ │ │ │  └── VAO = automotive qualification
│       │  │   │ │ │ └──── M6 = package code (e.g. QFN)
│       │  │   │ │ └────── E = temperature grade
│       │  │   │ └──────── T = tape & reel
│       │  │   └────────── 102 = pin count variant
│       │  └────────────── MP = sub-family (Motor/Power)
│       └───────────────── 64 = flash size (KB)
└───────────────────────── DSPIC33CK = device family
```

pickle strips the suffix after the base part number (e.g. `DSPIC33CK64MP102`) for device lookup.

## Device Family Packs (.atpack)

Microchip distributes device data as `.atpack` files — ZIP archives containing:

- `edc/*.PIC` — EDC XML files with pin/peripheral definitions
- `include/` — C header files
- `scripts/` — Linker scripts

pickle uses only the EDC XML files. The pack index at `https://packs.download.microchip.com/` lists all available packs with download URLs.

### EDC XML Structure

EDC files use the namespace `http://crownking/edc` and describe:

- **DCR (Device Configuration Registers)** — fuse registers and fields
- **PinList** — physical pins grouped by function
- **PPS (Peripheral Pin Select)** — remappable I/O group definitions

Each `<edc:Pin>` element contains pad name, port/bit, RP number, and function list.

## PPS (Peripheral Pin Select)

Most digital peripherals on dsPIC33CK/CH devices are not hardwired to specific pins. Instead, they use PPS — a crossbar switch configured at runtime.

### How PPS Works

1. **Unlock** the PPS lock bit: `__builtin_write_RPCON(0x0000U)`
2. **Write input mappings** to `RPINRn` registers — tells the peripheral which RP pin to listen to
3. **Write output mappings** to `RPORn` registers — tells the pin which peripheral signal to drive
4. **Lock** the PPS: `__builtin_write_RPCON(0x0800U)`

### Register Naming

- Input: `RPINR18bits.U1RXR = 36U` — UART1 RX reads from RP36
- Output: `RPOR1bits.RP37R = 1U` — RP37 drives peripheral function 1 (U1TX)

Each peripheral has a fixed `ppsval` (the value written to RPORn for outputs, or the register/field for inputs). These values come from the EDC XML.

### RP Pin Numbers

Each remappable pin has an RP number derived from its port position:
- RA0 = RP0, RA1 = RP1, ..., RB0 = RP32, RB1 = RP33, etc.
- Not all pins are remappable — power, MCLR, and some analog-only pins have no RP number.

## ICSP (In-Circuit Serial Programming)

dsPIC33 devices have up to 3 ICSP debug channel pairs:

| Channel | Clock Pin | Data Pin |
|---|---|---|
| 1 (default) | PGC1 | PGD1 |
| 2 | PGC2 | PGD2 |
| 3 | PGC3 | PGD3 |

The active channel is selected by `FICD.ICS` (configuration fuse). pickle detects ICSP pins by regex matching on pad function names and excludes them from TRIS/ANSEL code generation — only a reservation comment is emitted.

**MCLR** (Master Clear / Reset) is always reserved and never configured as GPIO.

## Configuration Fuses

dsPIC33 devices have non-volatile configuration registers set at programming time. In XC16, these are configured with `#pragma config`:

### FICD — ICD Configuration
- `ICS` — ICSP channel (1, 2, or 3)
- `JTAGEN` — JTAG port enable (ON/OFF)

### FWDT — Watchdog Timer
- `FWDTEN` — Watchdog enable (ON, OFF, SWON for software-controlled)
- `WDTPS` — Watchdog prescaler (PS1 through PS32768)

### FOSCSEL — Oscillator Selection
- `FNOSC` — Initial oscillator source (FRC, FRCPLL, PRI, PRIPLL, LPRC, etc.)
- `IESO` — Two-speed startup enable

### FOSC — Oscillator Configuration
- `POSCMD` — Primary oscillator mode (EC, XT, HS, NONE)
- `FCKSM` — Clock switching and monitor (CSDCMD = both disabled)

### FBORPOR — Brown-out Reset
- `BOREN` — Brown-out reset enable
- `BORV` — Brown-out voltage threshold

## Oscillator System

The dsPIC33CK oscillator system can use several clock sources:

```
              ┌─────────┐
  FRC (8MHz)──┤         ├──┐
              │   MUX   │  │    ┌─────┐    ┌─────┐
  Primary ────┤         ├──┴───>│ PLL │───>│ /2  │──> Fcy
  (EC/XT/HS)  │         │      └─────┘    └─────┘
              │         │        │
  LPRC (32k)──┤         │   Fvco = Fplli * M / N1
              └─────────┘   Fosc = Fvco / (N2 * N3)
                             Fcy  = Fosc / 2
```

### PLL Constraints (dsPIC33CK)

| Parameter | Min | Max |
|---|---|---|
| N1 (input divider) | 1 | 8 |
| M (multiplier) | 16 | 200 |
| N2 (post divider 1) | 1 | 7 |
| N3 (post divider 2) | 1 | 7 |
| Fvco | 400 MHz | 1.6 GHz |
| FPFD (phase detector) | 8 MHz | — |

pickle performs a brute-force search over all valid (N1, M, N2, N3) combinations to find the set that produces Fosc closest to the user's target, preferring exact matches.

## Port Registers

Each I/O port has several control registers:

| Register | Purpose | Bit = 1 |
|---|---|---|
| `TRISx` | Data direction | Input |
| `PORTx` | Read pin state | High |
| `LATx` | Output latch | High |
| `ANSELx` | Analog select | Analog mode |
| `ODCx` | Open-drain control | Open-drain enabled |

pickle generates writes to TRIS, LAT, ANSEL, and ODC based on peripheral assignments. Analog pins default to analog mode — ANSEL must be cleared explicitly for digital function.

## Pinout Overlays

Some package variants are not present in the EDC XML but are documented in datasheets. pickle supports manual or verified overlays stored as JSON in `pinouts/`:

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

These are loaded by `dfp_manager::load_pinout_overlays()` and merged into the device's pinout map at load time.
