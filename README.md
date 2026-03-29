# pickle

<p align="center">
  <img src="src-tauri/icons/pickle-icon.svg" width="128" height="128" alt="pickle logo — a pickle-shaped DIP IC chip">
</p>

**pickle** is a native desktop pin multiplexing configurator for Microchip dsPIC33 microcontrollers. It parses Device Family Pack (`.atpack`) files, presents an interactive pin assignment UI, and generates MISRA C:2012 compliant initialization code.

The name is a pun on **PIC** — Microchip's microcontroller line.

## Features

- Interactive pin table with real-time conflict detection
- Visual DIP / QFP / QFN package diagram
- PPS (Peripheral Pin Select) register generation with unlock/lock
- Oscillator PLL configuration with brute-force divider search
- Configuration fuse pragma generation
- Optional XC16 compile check and Anthropic API pinout verification
- Save/load pin assignments, export C files or pin lists

## Quick Start

```bash
# Prerequisites: Rust (https://rustup.rs), Tauri CLI (cargo install tauri-cli)

git clone https://github.com/jihlenburg/pickle.git
cd pickle

cargo tauri dev       # Dev mode with hot-reload
cargo tauri build     # Release build
./scripts/release-app.sh  # Release build + copy pickle.app into ./bin
```

### Tests

```bash
cd src-tauri && cargo test
```

## Documentation

See [`docs/`](docs/) for detailed documentation:

- [Architecture](docs/architecture.md) — project structure, data flow, module overview
- [Tauri Commands](docs/commands.md) — all 14 IPC commands, including native file dialog flows
- [Code Generation](docs/codegen.md) — how C code is generated, PPS, TRIS, ANSEL, oscillator, fuses
- [Domain Knowledge](docs/domain.md) — dsPIC33 PPS, ICSP, part numbering, config fuses

## License

MIT
