//! Device catalog and index-status commands.

use log::info;

use crate::parser::{dfp_manager, pack_index};

use super::{round_tenths, DeviceListResponse, IndexStatusResponse, RefreshResponse};

#[tauri::command]
pub fn list_devices() -> Result<DeviceListResponse, String> {
    info!("list_devices: enumerating devices");
    let cached = dfp_manager::list_cached_devices();
    let all_devs = dfp_manager::list_all_known_devices();
    info!(
        "list_devices: {} total, {} cached",
        all_devs.len(),
        cached.len()
    );
    Ok(DeviceListResponse {
        total: all_devs.len(),
        cached_count: cached.len(),
        devices: all_devs,
        cached,
    })
}

#[tauri::command]
pub fn refresh_index() -> Result<RefreshResponse, String> {
    let index = pack_index::get_pack_index(true)?;
    Ok(RefreshResponse {
        success: true,
        device_count: index.devices.len(),
        pack_count: index.packs.len(),
        age_hours: round_tenths(index.age_hours()),
    })
}

#[tauri::command]
pub fn index_status() -> Result<IndexStatusResponse, String> {
    match pack_index::get_pack_index(false) {
        Ok(index) => Ok(IndexStatusResponse {
            available: true,
            device_count: index.devices.len(),
            pack_count: index.packs.len(),
            age_hours: Some(round_tenths(index.age_hours())),
            is_stale: index.is_stale(),
        }),
        Err(_) => Ok(IndexStatusResponse {
            available: false,
            device_count: 0,
            pack_count: 0,
            age_hours: None,
            is_stale: true,
        }),
    }
}
