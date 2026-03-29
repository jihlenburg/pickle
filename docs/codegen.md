# Code Generation

pickle generates MISRA C:2012 compliant C99 initialization code for dsPIC33 pin configuration. The output is split into two files: `pin_config.h` and `pin_config.c`.

## Output Files

### pin_config.h

Contains:
- Include guard
- Part number comment
- `#include <xc.h>`
- Signal name `#define` macros (PORT, LAT, TRIS accessors)
- Function prototypes: `system_pin_init()`, `configure_pps()`, `configure_ports()`
- Oscillator and fuse function prototypes (if configured)

### pin_config.c

Contains:
- `#include "pin_config.h"`
- `configure_pps()` — PPS register writes with RPCON unlock/lock
- `configure_ports()` — TRIS, LAT, ANSEL, ODC register writes
- `system_pin_init()` — calls the above in correct order
- Oscillator init function (if configured)
- Fuse `#pragma config` directives (appended at file end)

## PPS (Peripheral Pin Select)

Remappable peripherals are configured by writing to RPINR (input) and RPOR (output) registers.

```c
void configure_pps(void) {
    __builtin_write_RPCON(0x0000U);  /* Unlock PPS */

    /* Inputs */
    RPINR18bits.U1RXR = 36U;        /* U1RX <- RB4 (RP36) */

    /* Outputs */
    RPOR1bits.RP37R   = 1U;         /* U1TX -> RB5 (RP37) */

    __builtin_write_RPCON(0x0800U);  /* Lock PPS */
}
```

The unlock (`0x0000U`) and lock (`0x0800U`) writes bracket all PPS register modifications. Inputs reference `RPINRn` registers; outputs reference `RPORn` registers with the peripheral's `ppsval`.

## Port Configuration

Port registers are written per-port using bitmasks:

```c
void configure_ports(void) {
    /* Port B */
    TRISB  |= 0x0010U;   /* RB4 input  (U1RX) */
    TRISB  &= ~0x0020U;  /* RB5 output (U1TX) */
    ANSELB &= ~0x0030U;  /* RB4, RB5 digital */
}
```

- **TRIS**: direction (1 = input, 0 = output)
- **LAT**: output latch (set before enabling output)
- **ANSEL**: analog select (cleared for digital function)
- **ODC**: open-drain control (set for I2C SDA/SCL)

## ICSP Pin Handling

ICSP debug pins (MCLR, PGCn, PGDn) are detected by regex and **excluded** from TRIS/ANSEL generation. Only a reservation comment is emitted:

```c
/* Pin 1: MCLR — reserved (ICSP) */
/* Pin 14: PGC1 — reserved (ICSP debug clock) */
/* Pin 15: PGD1 — reserved (ICSP debug data) */
```

The active ICSP channel is determined by the `FICD.ICS` fuse setting.

## Comment Alignment

All inline comments are aligned to a consistent column using `align_comments()`. This produces clean, readable output:

```c
RPINR18bits.U1RXR = 36U;        /* U1RX <- RB4 (RP36) */
RPOR1bits.RP37R   = 1U;         /* U1TX -> RB5 (RP37) */
RPINR20bits.SCK1R = 44U;        /* SCK1 <- RB12 (RP44) */
```

## Signal Name Macros

When users assign signal names, the header generates accessor macros:

```c
/* ---- Signal name macros ---- */
#define nRESET_PORT    PORTBbits.RB0
#define nRESET_LAT     LATBbits.LATB0
#define nRESET_TRIS    TRISBbits.TRISB0
```

## Oscillator Configuration

The PLL calculator uses brute-force search over all valid divider combinations:

| Parameter | Range | Description |
|---|---|---|
| N1 | 1–8 | Input divider (pre-PLL) |
| M | 16–200 | PLL multiplier |
| N2 | 1–7 | Post-PLL divider 1 |
| N3 | 1–7 | Post-PLL divider 2 |

**Constraints:**
- VCO frequency: 400 MHz – 1.6 GHz
- Phase detector input (FPFD): >= 8 MHz
- Formula: `Fosc = (Fplli * M) / (N1 * N2 * N3)`

**Supported clock sources:**
| Source | Description |
|---|---|
| `frc` | Fast RC oscillator (8 MHz), no PLL |
| `frc_pll` | FRC through PLL |
| `pri` | Primary oscillator (EC/XT/HS), no PLL |
| `pri_pll` | Primary oscillator through PLL |
| `lprc` | Low-Power RC (32 kHz) |

Output is a pair of `#pragma config` lines and an init function with register writes.

## Fuse Configuration

Generated as `#pragma config` directives:

```c
/* ---- Configuration Fuses ---- */

/* FICD: ICD Configuration */
#pragma config ICS = 1            /* ICSP channel PGC1/PGD1 */
#pragma config JTAGEN = OFF       /* JTAG disabled */

/* FWDT: Watchdog Timer */
#pragma config FWDTEN = OFF       /* Watchdog disabled */
#pragma config WDTPS = PS1024     /* WDT prescaler 1:1024 */

/* FBORPOR: Brown-out Reset */
#pragma config BOREN = ON         /* Brown-out reset enabled */
#pragma config BORV = BOR_HIGH    /* Brown-out voltage high */
```

Supported fuse fields:

| Fuse | Register | Options |
|---|---|---|
| ICS | FICD | 1, 2, 3 (ICSP channel) |
| JTAGEN | FICD | ON, OFF |
| FWDTEN | FWDT | ON, OFF, SWON |
| WDTPS | FWDT | PS1 – PS32768 |
| BOREN | FBORPOR | ON, OFF |
| BORV | FBORPOR | BOR_HIGH, BOR_MID, BOR_LOW |
