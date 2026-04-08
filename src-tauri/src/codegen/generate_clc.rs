//! CLC configuration packing and emitted-code helpers.
//!
//! CLC generation has its own backend data shape, register packing rules, and
//! family split between classic split-register parts and dsPIC33AK unified
//! registers. Pulling that logic out of `generate.rs` keeps the main generator
//! focused on phase orchestration rather than CLC-specific bit packing.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::codegen::generate_support::{align_comments, push_section_comment};
use crate::part_profile::PartProfile;

#[derive(Debug, Clone, Copy)]
struct ClcRegisterValues {
    conl: u16,
    conl_enable: u16,
    conh: u16,
    sel: u16,
    glsl: u16,
    glsh: u16,
}

#[derive(Debug, Clone, Copy)]
struct ClcAkRegisterValues {
    con: u32,
    con_enable: u32,
    sel: u32,
    gls: u32,
}

fn compute_clc_register_values(config: &ClcModuleConfig) -> ClcRegisterValues {
    let mut conl = (config.mode & 0x7) as u16;
    if config.lcpol {
        conl |= 1 << 5;
    }
    if config.lcoe {
        conl |= 1 << 7;
    }
    if config.intn {
        conl |= 1 << 10;
    }
    if config.intp {
        conl |= 1 << 11;
    }

    let mut conh = 0;
    for gate in 0..4 {
        if config.gpol[gate] {
            conh |= 1 << gate;
        }
    }

    let sel = (config.ds[0] as u16 & 0x7)
        | ((config.ds[1] as u16 & 0x7) << 4)
        | ((config.ds[2] as u16 & 0x7) << 8)
        | ((config.ds[3] as u16 & 0x7) << 12);

    let mut glsl = 0;
    for bit in 0..8 {
        if config.gates[0][bit] {
            glsl |= 1 << bit;
        }
        if config.gates[1][bit] {
            glsl |= 1 << (bit + 8);
        }
    }

    let mut glsh = 0;
    for bit in 0..8 {
        if config.gates[2][bit] {
            glsh |= 1 << bit;
        }
        if config.gates[3][bit] {
            glsh |= 1 << (bit + 8);
        }
    }

    ClcRegisterValues {
        conl,
        conl_enable: conl | (1 << 15),
        conh,
        sel,
        glsl,
        glsh,
    }
}

fn compute_clc_register_values_ak(config: &ClcModuleConfig) -> ClcAkRegisterValues {
    let mut con = (config.mode & 0x7) as u32;
    if config.lcpol {
        con |= 1 << 5;
    }
    if config.lcoe {
        con |= 1 << 7;
    }
    if config.intn {
        con |= 1 << 10;
    }
    if config.intp {
        con |= 1 << 11;
    }
    for gate in 0..4 {
        if config.gpol[gate] {
            con |= 1 << (16 + gate);
        }
    }

    let sel = (config.ds[0] as u32 & 0x7)
        | ((config.ds[1] as u32 & 0x7) << 4)
        | ((config.ds[2] as u32 & 0x7) << 8)
        | ((config.ds[3] as u32 & 0x7) << 12);

    // The unified dsPIC33AK CLCxGLS register stores each gate in an 8-bit slice.
    // Within each slice the negated term is the low bit and the true term is the
    // high bit for each data input pair.
    let mut gls = 0_u32;
    for gate in 0..4 {
        let base = gate * 8;
        if config.gates[gate][0] {
            gls |= 1 << (base + 1);
        }
        if config.gates[gate][1] {
            gls |= 1 << base;
        }
        if config.gates[gate][2] {
            gls |= 1 << (base + 3);
        }
        if config.gates[gate][3] {
            gls |= 1 << (base + 2);
        }
        if config.gates[gate][4] {
            gls |= 1 << (base + 5);
        }
        if config.gates[gate][5] {
            gls |= 1 << (base + 4);
        }
        if config.gates[gate][6] {
            gls |= 1 << (base + 7);
        }
        if config.gates[gate][7] {
            gls |= 1 << (base + 6);
        }
    }

    ClcAkRegisterValues {
        con,
        con_enable: con | (1 << 15),
        sel,
        gls,
    }
}

/// Configuration for a single CLC module (`CLCn` for the selected device).
/// Field values map directly to register bits as documented in DS70005298A.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClcModuleConfig {
    /// Data Selection MUX values (DS1-DS4), 3 bits each
    pub ds: [u8; 4],
    /// Gate source enable bits: gates[gate_idx][bit_idx]
    /// Bit order per gate: D1T, D1N, D2T, D2N, D3T, D3N, D4T, D4N
    pub gates: [[bool; 8]; 4],
    /// Gate polarity inversion (G1POL-G4POL)
    pub gpol: [bool; 4],
    /// Logic function mode (MODE<2:0>, 0-7)
    pub mode: u8,
    /// Output polarity inversion (LCPOL)
    pub lcpol: bool,
    /// Output enable to pin (LCOE)
    pub lcoe: bool,
    /// Module enable (LCEN)
    pub lcen: bool,
    /// Interrupt on positive edge (INTP)
    pub intp: bool,
    /// Interrupt on negative edge (INTN)
    pub intn: bool,
}

const CLC_MODE_NAMES: [&str; 8] = [
    "AND-OR",
    "OR-XOR",
    "4-input AND",
    "S-R Latch",
    "1-input D flip-flop with S/R",
    "2-input D flip-flop with R",
    "J-K flip-flop with R",
    "Transparent latch with S/R",
];

pub(crate) fn append_configure_clc_function(
    lines: &mut Vec<String>,
    clc_mods: &HashMap<u32, ClcModuleConfig>,
    part_number: &str,
) {
    let profile = PartProfile::from_part_number(part_number);
    let dspic33ak = profile.is_dspic33ak();
    push_section_comment(
        lines,
        "configure_clc",
        &[
            "",
            if dspic33ak {
                "dsPIC33AK parts use unified 32-bit CLCxCON / CLCxSEL / CLCxGLS"
            } else {
                "Configures the Configurable Logic Cell modules. Each module is disabled"
            },
            if dspic33ak {
                "registers. Each module is disabled before configuration and enabled last."
            } else {
                "before writing its configuration registers, then enabled last."
            },
        ],
    );
    lines.push("void configure_clc(void)".into());
    lines.push("{".into());

    let mut sorted_keys: Vec<_> = clc_mods.keys().collect();
    sorted_keys.sort();

    for (i, idx) in sorted_keys.iter().enumerate() {
        let mod_cfg = &clc_mods[idx];
        let n = idx;
        let mode_name = CLC_MODE_NAMES
            .get(mod_cfg.mode as usize)
            .unwrap_or(&"Unknown");

        let mut clc_lines = Vec::new();
        clc_lines.push(format!("    /* CLC{} — {} */", n, mode_name));
        if dspic33ak {
            let registers = compute_clc_register_values_ak(mod_cfg);
            clc_lines.push(format!(
                "    CLC{}CON = 0x00000000U; /* Disable module before configuration */",
                n
            ));
            clc_lines.push(format!(
                "    CLC{}SEL = 0x{:08X}U; /* DS1={}, DS2={}, DS3={}, DS4={} */",
                n, registers.sel, mod_cfg.ds[0], mod_cfg.ds[1], mod_cfg.ds[2], mod_cfg.ds[3]
            ));
            clc_lines.push(format!(
                "    CLC{}GLS = 0x{:08X}U; /* Gate source enables */",
                n, registers.gls
            ));
            if mod_cfg.lcen {
                clc_lines.push(format!(
                    "    CLC{}CON = 0x{:08X}U; /* Enable: MODE={}, LCOE={}, LCPOL={} */",
                    n,
                    registers.con_enable,
                    mod_cfg.mode,
                    if mod_cfg.lcoe { "on" } else { "off" },
                    if mod_cfg.lcpol { "inv" } else { "norm" }
                ));
            } else {
                clc_lines.push(format!(
                    "    CLC{}CON = 0x{:08X}U; /* Module disabled */",
                    n, registers.con
                ));
            }
        } else {
            let registers = compute_clc_register_values(mod_cfg);
            clc_lines.push(format!(
                "    CLC{}CONL = 0x0000U; /* Disable module before configuration */",
                n
            ));
            clc_lines.push(format!(
                "    CLC{}SEL  = 0x{:04X}U; /* DS1={}, DS2={}, DS3={}, DS4={} */",
                n, registers.sel, mod_cfg.ds[0], mod_cfg.ds[1], mod_cfg.ds[2], mod_cfg.ds[3]
            ));
            clc_lines.push(format!(
                "    CLC{}GLSL = 0x{:04X}U; /* Gate 1-2 source enables */",
                n, registers.glsl
            ));
            clc_lines.push(format!(
                "    CLC{}GLSH = 0x{:04X}U; /* Gate 3-4 source enables */",
                n, registers.glsh
            ));
            clc_lines.push(format!(
                "    CLC{}CONH = 0x{:04X}U; /* Gate polarity */",
                n, registers.conh
            ));
            if mod_cfg.lcen {
                clc_lines.push(format!(
                    "    CLC{}CONL = 0x{:04X}U; /* Enable: MODE={}, LCOE={}, LCPOL={} */",
                    n,
                    registers.conl_enable,
                    mod_cfg.mode,
                    if mod_cfg.lcoe { "on" } else { "off" },
                    if mod_cfg.lcpol { "inv" } else { "norm" }
                ));
            } else {
                clc_lines.push(format!(
                    "    CLC{}CONL = 0x{:04X}U; /* Module disabled */",
                    n, registers.conl
                ));
            }
        }

        lines.extend(align_comments(&clc_lines));
        if i + 1 < sorted_keys.len() {
            lines.push(String::new());
        }
    }

    lines.push("}".into());
    lines.push(String::new());
}
