# dsPIC33 / PIC24 Notes

This page captures the device-level concepts that matter to pickle's parser and generator.

## Part Numbers

### Structure

```text
dsPIC33  ff  mmmm  xx  r  pp  T/P
│        │   │     │   │  │   └── temperature grade / package code
│        │   │     │   │  └───── pin count
│        │   │     │   └──────── feature set (unique family identifier)
│        │   │     └──────────── type (application class)
│        │   └────────────────── program memory size (KB)
│        └────────────────────── performance family
└─────────────────────────────── device family
```

Example: `DSPIC33CK64MP102T-E/M6VAO`

### ff — Performance Family

| Code | Description |
|------|-------------|
| AK   | Up to 200 MHz, 32-bit Single Core, DP-FPU, 3.3V |
| CH   | Up to 90+100 MHz, Dual Core, 3.3V |
| CK   | Up to 100 MHz, Single Core, 3.3V |
| CDVL | Up to 100 MHz, Single Core, 3.3V, Integrated MOSFET Driver and LIN Transceiver |
| EDV  | Up to 70 MHz, Single Core, 3.3V, Integrated MOSFET Driver |
| EV   | Up to 70 MHz, Single Core, 5V |
| EP   | Up to 70 MHz, Single Core, 3.3V |

### mmmm — Program Memory Size

| Code | Size |
|------|------|
| 32   | 32 KB |
| 64   | 64 KB |
| 128  | 128 KB |
| 256  | 256 KB |
| 512  | 512 KB |
| 1024 | 1024 KB |

### xx — Type (Application Class)

| Code   | Description |
|--------|-------------|
| MPT    | Motor Control, Power Supply, General Purpose and Robust Design with integrated Security |
| MP/GS  | Motor Control, Power Supply, General Purpose and Robust Design |
| MC/GM  | Motor Control, Cost-effective General Purpose and Robust Design |
| GP     | Cost-effective General Purpose and Robust Design |
| MU     | Motor Control, General Purpose and Robust Design with USB |

### r — Feature Set

Unique family identifier. Higher the number, richer the feature set.

### pp — Pin Count

| Code | Pins |
|------|------|
| 02   | 28 |
| 03   | 36 |
| 04   | 44 |
| 05   | 48 |
| 06   | 64 |
| 08   | 80 |
| 10   | 100 |
| 14   | 144 |

### T — Temperature Grade

| Code | Range |
|------|-------|
| I    | Industrial (−40°C to +85°C) |
| E    | Extended (−40°C to +125°C) |
| H    | High (−40°C to +150°C) |

### P — Package Code

| Code | Package |
|------|---------|
| PT   | TQFP |
| MM/MV/ML | QFN variants |
| MR/MQ/M6/MX/M2 | QFN variants |
| M5/M4 | QFN/uQFN/VQFN |
| SS   | SSOP |
| SO   | SOIC |

pickle normalizes lookup keys to the base part number (up to and including
the pin-count code), e.g. `DSPIC33CK64MP102`.

### Sibling devices

Devices sharing the same type+feature-set+pin-count suffix (e.g. `MC106`)
have the same pinout and peripheral set. They differ only in flash size
(`mmmm`) and performance family (`ff`). This means a datasheet for
`dsPIC33CDVL64MC106` covers the pin table for `dsPIC33CDV128MC106` as well.

Note from the selection guide: "Similar family of devices with fewer
variations are grouped with the same color coding."

## Product Family Overview

Source: dsPIC33 DSC Product Selection Guide (DS30010244H, March 2026).

### dsPIC33AK — High-Performance 32-bit

- 200 MHz, 32-bit CPU with DP-FPU
- Flash: 32–512 KB, RAM: up to 256 KB
- Pin counts: 28–128
- 12-bit ADC (up to 12 channels), 12-bit DAC
- Hardware Safety Level: L5
- CLC, PPS, PTG, DMA available

### dsPIC33CH — Dual Core

- Main core 90–100 MHz, Secondary core 24–72 MHz
- Flash: 64–1024 KB (main), RAM: 16–128 KB
- Pin counts: 28–80
- Hardware Safety Level: L3–L4
- CLC, PPS, PTG, DMA available

### dsPIC33CK — Single Core

- 100 MHz, single core
- Flash: 32–512 KB, RAM: 8–64 KB
- Pin counts: 28–100
- Includes Secure DSC variant (dsPIC33CK512MPT608) with cryptographic
  accelerator, CodeGuard Security, HSM
- Hardware Safety Level: L4
- CLC, PPS, PTG, DMA available

### dsPIC33CDV/CDVL — Motor Control with Integrated MOSFET Gate Drivers

- 100 MHz, single core
- **CDV**: Integrated MOSFET Driver
- **CDVL**: Integrated MOSFET Driver **and** LIN Transceiver
- Flash: 64–256 KB, RAM: 8–32 KB
- Pin count: 64
- 16-bit PWM resolution for `CoxxMC` and `xxxMC` variants
- Hardware Safety Level: L4
- CLC, PPS, DMA available

### dsPIC33EDV — Motor Control with Integrated MOSFET Gate Drivers (70 MHz)

- 70 MHz, single core, 3.3V
- Flash: 64 KB, RAM: 8 KB
- Pin count: 52
- PWM resolution: 7.14 ns
- Hardware Safety Level: L2

### dsPIC33EV — 5V Single Core

- 70 MHz, single core, **5V operating voltage**
- Flash: 32–256 KB, RAM: 4–16 KB
- Pin counts: 28–64
- Hardware Safety Level: L3
- CLC, PPS, PTG, DMA available

### dsPIC33EP — 3.3V Single Core

- 70 MHz, single core, 3.3V
- Flash: 16–512 KB, RAM: 2–52 KB
- Pin counts: 28–144
- Hardware Safety Level: L1–L2
- CLC, PPS, PTG, DMA available
- Includes USB (MU) variants

## Hardware Safety Levels

The selection guide groups safety features into progressive levels:

| Level | Features |
|-------|----------|
| L1    | WDT, oscillator fail-safe, illegal opcode detect, TRAP, reset trace, register lock, frequency check, CodeGuard security, PWM lock* |
| L2    | L1 + CRC |
| L3    | L2 + Flash ECC and/or DMT |
| L4    | L3 + RAM MBIST |
| L5    | L4 + ECC RAM + IOIM |

\*PWM lock available in devices with MC PWM/SMPS PWM peripheral (5V
dsPIC33 DSCs with 5V operating voltage).

## Peripheral Abbreviations

Reference for the column headers in the dsPIC33 DSC Product Selection
Guide and for understanding peripheral names in EDC data.

### Integrated Analog

| Abbreviation | Full Name | Description |
|---|---|---|
| ADC | Analog-to-Digital Converter | General-purpose ADC with up to 10-/12-bit resolution |
| HS ADC | High-Speed ADC | High-speed SAR ADC with 12-bit resolution and sampling speed of 10 Msps |
| DAC | Digital-to-Analog Converter | General-purpose DAC with resolution up to 16-bit |
| ΔΣ DAC | Delta-Sigma DAC | Second-order digital bipolar, two output channel Delta-Sigma DAC with stereo operation support |
| HS Comp | High-Speed Comparator | General-purpose rail-to-rail comparator with <1 ns response time |
| OPA/PGA | Op Amp / Programmable Gain Amplifier | General-purpose op amp and PGAs for internal and external signal source conditioning |

### Waveform Control

| Abbreviation | Full Name | Description |
|---|---|---|
| SCCP | Single Capture/Compare/PWM | Multi-purpose 16-/32-bit input capture, output compare and PWM |
| MCCP | Multiple Capture/Compare/PWM | Multi-purpose 16-/32-bit input capture, output compare and PWM with up to six outputs and an extended range of output control features |
| PWM | Pulse Width Modulation | 16-bit PWM with up to nine independent time bases |
| MC PWM | Motor Control PWM | Motor control 16-bit PWM with multiple synchronized pulse-width modulation, up to six outputs with four duty cycle generators and resolution up to 1 ns |
| SMPS PWM | Power Supply Pulse Width Modulation | Power supply 16-bit PWM with multiple synchronized pulse-width modulation, up to eight outputs with four independent time bases and resolution up to 1 ns |
| IC | Input Capture | Input capture with an independent timer base to capture an external event |
| OC | Output Compare | Output compare with an independent time base to compare value with compare registers and generate a single output pulse, or a train of output pulses on a compare match event |

### Timers and Interfaces

| Abbreviation | Description |
|---|---|
| Host BiSS Interface | Host bidirectional Serial Synchronous (BiSS) digital interface for actuators used in position control |
| 16-/32-bit Timer | General-purpose 16-/32-bit timer/counter with compare capability |
| QEI | Quadrature Encoder Interface — increment encoders for obtaining mechanical position data |

### Safety and Monitoring

| Abbreviation | Description |
|---|---|
| ECC | Error Correction Code — detects single and double bit errors, corrects single bit error automatically |
| RAM MBIST | RAM Memory Built-In Self-Test — tests functional correctness of all memory locations |
| WDT | Watch Dog Timer — system supervisory circuit that generates a reset when software timing anomalies are detected within a configurable critical window |
| DMT | Dead Man Timer — system supervisory circuit that generates a reset when instruction sequence anomalies are detected within a configurable critical window |
| CRC | Cyclic Redundancy Check with Memory Scan — automatically calculates CRC checksum of Program/DataEE memory for NVM integrity and a general-purpose 16-bit CRC for use with memory and communications data |
| Core Voltage Monitor | Hardware monitor that supervises the internal core voltage and flags abnormal conditions to support functional safety and system reliability |
| IOIM | IO Integrity Monitors — validates the IO functionality in safety-critical applications by checking an output signal against a reference |
| Hardware Safety Features | Flash error correction, RAM MBIST, backup system oscillator, WDT, DMT, CRC scan, etc. |
| Functional Safety (ISO 26262 / IEC 61508) | Functional Ready Devices are ideal for automotive and industrial safety applications requiring ISO 26262 (ASIL B/C) and IEC 61508 (SIL 2/3) safety compliance |
| IEC 60730 Class B Safety | Class B safety diagnostic libraries for designing household applications |

### Communications

| Abbreviation | Description |
|---|---|
| USB OTG | USB 2.0 full-speed (host and device), low-speed (host) and On-The-Go (OTG) support |
| CAN/CAN FD | Controller Area Network — industrial- and automotive-centric communication bus |
| UART | Universal Asynchronous Receiver Transmitter — full-duplex, 8-bit or 9-bit data serial communications with optional ISO 7816 Smart Card support |
| LIN | Local Interconnect Network — industrial- and automotive-centric (support for LIN when using the EUSART) |
| I²C | Inter-Integrated Circuit — general purpose 2-wire IC serial interface for communicating with other peripherals or microcontroller devices |
| IIC | Improved Inter-Integrated Circuit — multi-controller serial data communication interface to communicate with the controller or the target |
| SPI | Serial Peripheral Interface — general-purpose 4-wire synchronous serial interface with other peripherals or microcontroller devices |
| I²S | Data Converter Interface — 3-wire synchronous half duplex serial interface to handle the stereo data |
| SENT | Single-Edge Nibble Transmission — unidirectional, single-wire serial communications protocol designed for point-to-point transmission of signal values |

### User Interface

| Abbreviation | Description |
|---|---|
| Hardware Core Independent Touch Sensing | Hardware Core Independent Touch implemented using PTG and high-speed ADCs — enables the implementation of touch buttons, sliders, wheels, pads, etc. |

### Security

| Abbreviation | Description |
|---|---|
| Security Access Control | Secure boot, secure firmware update, secure debug access control, code protection and device locking |
| Crypto Accelerator | Dedicated hardware engine to accelerate cryptographic operations, improving system security, performance, and reduced latency |
| CodeGuard Security — Secure Boot | Allows devices to configure the boot segment as a read-only section of memory to protect the bootloader from modification via remote digital attacks |
| Flash OTP by ICSP Write Inhibit | Flash OTP by ICSP Write Inhibit enables Flash to be configured as One-Time Programmable (OTP) memory with the ability to write and read protect the Flash memory |
| HSM | Integrated Secure Subsystem — supports implementing secure boot, Message Authentication, trusted firmware updates, mutual node authentication and multiple key management protocols |

### System Flexibility

| Abbreviation | Description |
|---|---|
| Dual Partition Flash | Dual partition Flash operation, allowing the support of robust bootloader systems and fail-safe storage of application code, with options designed to enhance code security |
| CLC | Configurable Logic Cell — integrated combinational and sequential logic with custom interconnection and re-routing of digital peripherals |
| PPS | Peripheral Pin Select — I/O pin remapping of digital peripherals for greater design flexibility and improved EMI board layout |
| PTG | Peripheral Trigger Generator — user-programmable sequencer, capable of generating complex trigger signal sequences to coordinate the operation of other peripherals |
| DMA | Direct Memory Access — direct memory access for transfer of data between the CPU and its peripherals without CPU assistance |
| IDLE, SLEEP and PMD | Low-power saving modes |

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

## Fuse-Driven I2C Routing

Some dsPIC33/PIC24 parts expose alternate fixed I2C pads controlled by
configuration fuses such as `ALTI2C1` and `ALTI2C2`.

Key points for pickle:

- `SCLx` / `SDAx` and `ASCLx` / `ASDAx` are mutually exclusive routed aliases
- the frontend hides inactive aliases from the normal assignment UI
- when an `ALTI2Cx` fuse flips, pickle re-routes fixed I2C assignments to the
  active pins when those pins exist on the current package
- if the selected alternate route is not bonded out on the current package, the
  affected assignments are stashed until the route becomes valid again

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

The per-gate true/complement inputs are independent literals. If both `DnT`
and `DnN` are enabled for the same gate, pickle treats them as two distinct
active input paths rather than collapsing them into one visual connection.

The schematic preview uses ANSI-style orthogonal routing for those enabled
literals. Different nets may cross when necessary, but they must never overlap
on the same colinear wire segment.

See [CLC](clc.md) for the full designer, persistence, routing, and codegen
contract.

### Source mapping priority

CLC source labels are resolved in this order:

1. `clc_sources/<PART>.json`
2. LLM-extracted data saved from verification
3. hardcoded fallback mappings for known modules
4. generic `CLCINn` labels when nothing better exists
