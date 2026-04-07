//! Device loading and code-generation commands.

use log::{error, info};
use serde_json::Value;
use std::collections::HashMap;

use crate::codegen::fuses::generate_dynamic_fuse_pragmas;
use crate::codegen::generate::{
    generate_c_files_named, ClcModuleConfig, GenerateOutputOptions, PinAssignment, PinConfig,
};
use crate::codegen::oscillator::OscConfig;
use crate::parser::dfp_manager;
use crate::settings;

use super::{device_packages, parse_u32_keyed_map, selected_package, CodegenRequest};

#[tauri::command]
pub fn load_device(part_number: String, package: Option<String>) -> Result<Value, String> {
    info!("load_device: part={} package={:?}", part_number, package);
    let device = dfp_manager::load_device(&part_number).ok_or_else(|| {
        error!("load_device: device {} not found", part_number);
        format!("Device {} not found", part_number)
    })?;

    let selected_pkg = selected_package(&device, package.as_deref());

    let resolved_pins = device.resolve_pins(Some(selected_pkg));
    let pinout = device.get_pinout(Some(selected_pkg));

    info!(
        "load_device: {} loaded, package={}, {} pins",
        device.part_number, selected_pkg, pinout.pin_count
    );
    Ok(serde_json::json!({
        "part_number": device.part_number,
        "selected_package": selected_pkg,
        "packages": device_packages(&device),
        "pin_count": pinout.pin_count,
        "has_clc": device.has_clc(),
        "pins": resolved_pins,
        "remappable_inputs": device.remappable_inputs,
        "remappable_outputs": device.remappable_outputs,
        "pps_input_mappings": device.pps_input_mappings,
        "pps_output_mappings": device.pps_output_mappings,
        "port_registers": device.port_registers,
        "fuse_defs": device.fuse_defs,
        "clc_input_sources": device.clc_input_sources,
        "device_info": device.device_info,
    }))
}

#[tauri::command]
pub fn generate_code(request: CodegenRequest) -> Result<Value, String> {
    info!(
        "generate_code: part={} assignments={}",
        request.part_number,
        request.assignments.len()
    );
    let device = dfp_manager::load_device(&request.part_number).ok_or_else(|| {
        error!("generate_code: device {} not found", request.part_number);
        format!("Device {} not found", request.part_number)
    })?;

    let pkg_name = request.package.as_deref().unwrap_or(&device.default_pinout);

    let config = PinConfig {
        part_number: request.part_number.clone(),
        assignments: request
            .assignments
            .into_iter()
            .map(|a| PinAssignment {
                pin_position: a.pin_position,
                rp_number: a.rp_number,
                peripheral: a.peripheral,
                direction: a.direction,
                ppsval: a.ppsval,
                fixed: a.fixed,
            })
            .collect(),
        digital_pins: request.digital_pins,
    };

    let sig_names: HashMap<u32, String> = request
        .signal_names
        .into_iter()
        .filter_map(|(k, v)| k.parse::<u32>().ok().map(|k| (k, v)))
        .collect();

    let osc = request
        .oscillator
        .filter(|o| !o.source.is_empty())
        .map(|o| OscConfig {
            source: o.source,
            target_fosc_hz: (o.target_fosc_mhz * 1_000_000.0) as u64,
            crystal_hz: (o.crystal_mhz * 1_000_000.0) as u64,
            poscmd: o.poscmd,
        });

    let fuse_pragmas = request
        .fuses
        .map(|f| generate_dynamic_fuse_pragmas(&device.fuse_defs, &f.selections));

    let clc_modules: Option<HashMap<u32, ClcModuleConfig>> = request.clc.map(parse_u32_keyed_map);
    let output_basename = settings::load()
        .map(|settings| settings.codegen.output_basename)
        .unwrap_or_else(|_| settings::default_codegen_output_basename());

    let files = generate_c_files_named(
        &device,
        &config,
        Some(&sig_names),
        osc.as_ref(),
        fuse_pragmas.as_deref(),
        clc_modules.as_ref(),
        GenerateOutputOptions {
            package: Some(pkg_name),
            output_basename: &output_basename,
        },
    );

    Ok(serde_json::json!({ "files": files }))
}
