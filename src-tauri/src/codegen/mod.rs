//! C code generation modules for emitted pin, fuse, oscillator, and CLC output.

pub mod fuses;
pub mod generate;
pub(crate) mod generate_clc;
pub(crate) mod generate_ports;
pub(crate) mod generate_pps;
pub(crate) mod generate_single_file;
pub(crate) mod generate_support;
pub(crate) mod generate_types;
pub mod oscillator;
