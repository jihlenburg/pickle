# Tauri Commands

pickle exposes IPC commands via `#[tauri::command]` in `src-tauri/src/commands.rs`. The frontend calls them with `window.__TAURI__.core.invoke('command_name', { args })`.

## Device Management

### `list_devices`

List all known device part numbers (cached locally + from Microchip pack index).

**Parameters:** none

**Response:**
```json
{
  "devices": ["DSPIC33CK64MP102", "DSPIC33CK256MP508", ...],
  "cached": ["DSPIC33CK64MP102"],
  "total": 347,
  "cached_count": 1
}
```

### `load_device`

Load device data and resolve pins for a specific package variant.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `partNumber` | string | yes | e.g. `"DSPIC33CK64MP102"` |
| `package` | string | no | Package name; defaults to device's default pinout |

**Response:** JSON object with `part_number`, `selected_package`, `packages`, `pin_count`, `pins` (array of resolved pins), `remappable_inputs`, `remappable_outputs`, `pps_input_mappings`, `pps_output_mappings`, `port_registers`, `fuse_defs` (available configuration fuse fields and values), `clc_input_sources` (device-specific CLC input source mapping, if available).

### `refresh_index`

Re-fetch the Microchip pack index XML from the internet and update the local cache.

**Parameters:** none

**Response:**
```json
{
  "success": true,
  "device_count": 347,
  "pack_count": 42,
  "age_hours": 0.0
}
```

### `index_status`

Check current pack index availability and staleness without fetching.

**Parameters:** none

**Response:**
```json
{
  "available": true,
  "device_count": 347,
  "pack_count": 42,
  "age_hours": 12.5,
  "is_stale": false
}
```

## Behavior Settings

These commands back the shared `settings.toml` file stored under the platform app data directory.

### `load_app_settings`

Load the current behavior settings, creating the file with defaults if it does not exist yet.

**Parameters:** none

**Response:**
```json
{
  "path": "/Users/me/Library/Application Support/pickle/settings.toml",
  "settings": {
    "appearance": { "theme": "dark" },
    "startup": { "device": "last-used", "package": "" },
    "last_used": { "part_number": "DSPIC33CK64MP102", "package": "TQFP-28" }
  }
}
```

### `set_theme_mode`

Persist the theme mode used by the frontend toggle.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `theme` | string | yes | One of `"dark"`, `"light"`, or `"system"` |

**Response:** none

### `remember_last_used_device`

Update the `last_used` device/package after a successful load so `startup.device = "last-used"` can reopen it on the next launch.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `partNumber` | string | yes | Canonical part number |
| `package` | string | no | Last selected package |

**Response:** none

## Native File Dialogs

These commands back the desktop-native open/save/export flows used by the Tauri frontend.

### `open_text_file_dialog`

Open a text file with the platform file picker and return its contents.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `title` | string | no | Dialog title |
| `filters` | array | no | File filters, each with `name` and `extensions` |

**Response:**
```json
{
  "path": "/Users/me/config.json",
  "contents": "{\n  \"part_number\": \"DSPIC33CK64MP102\"\n}"
}
```

### `open_binary_file_dialog`

Open a binary file with the platform file picker and return its bytes as base64.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `title` | string | no | Dialog title |
| `filters` | array | no | File filters, each with `name` and `extensions` |

**Response:**
```json
{
  "path": "/Users/me/datasheet.pdf",
  "name": "datasheet.pdf",
  "base64": "JVBERi0xLjcK..."
}
```

### `save_text_file_dialog`

Open a save dialog and write a UTF-8 text file to the selected destination.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `title` | string | no | Dialog title |
| `suggestedName` | string | yes | Default filename shown in the save dialog |
| `contents` | string | yes | File contents |
| `filters` | array | no | File filters, each with `name` and `extensions` |

**Response:**
```json
{
  "path": "/Users/me/DSPIC33CK64MP102_28-pin SSOP.json"
}
```

### `export_generated_files_dialog`

Open a folder picker and write multiple generated files into the selected directory.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `title` | string | no | Dialog title |
| `files` | object | yes | Filename to contents mapping |

**Response:**
```json
{
  "directory": "/Users/me/Desktop/pickle-export",
  "writtenFiles": [
    "/Users/me/Desktop/pickle-export/pin_config.c",
    "/Users/me/Desktop/pickle-export/pin_config.h"
  ]
}
```

## Code Generation

### `generate_code`

Generate C source files from the current pin configuration.

**Parameters** (as `CodegenRequest`):
| Name | Type | Required | Description |
|---|---|---|---|
| `partNumber` | string | yes | Target device |
| `package` | string | no | Package variant |
| `assignments` | array | yes | Pin assignments (see below) |
| `signalNames` | object | no | `{ "pin_position": "signal_name" }` |
| `digitalPins` | array | no | Pin positions forced to digital mode |
| `oscillator` | object | no | Oscillator config (source, targetFoscMhz, crystalMhz, poscmd) |
| `fuses` | object | no | Fuse config (ics, jtagen, fwdten, wdtps, boren, borv) |
| `clc` | array | no | CLC module configs, each with `module` (1-4), `logicMode`, `dataSources` (4 DS indices), `gates` (4x4 T/N matrix), `polarities` |

Each assignment:
```json
{
  "pinPosition": 5,
  "rpNumber": 36,
  "peripheral": "U1RX",
  "direction": "in",
  "ppsval": 1,
  "fixed": false
}
```

**Response:**
```json
{
  "files": {
    "pin_config.c": "/* generated code ... */",
    "pin_config.h": "#ifndef PIN_CONFIG_H ..."
  }
}
```

### `compiler_info`

Check whether the XC16 compiler is installed and accessible.

**Parameters:** none

**Response:**
```json
{
  "available": true,
  "path": "/Applications/microchip/xc16/v2.10/bin/xc16-gcc",
  "version": "XC16 v2.10"
}
```

### `compile_check`

Test-compile generated C code using the local XC16 installation.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `code` | string | yes | C source code |
| `header` | string | no | Header file content |
| `partNumber` | string | yes | Target device (for `-mcpu` flag) |

**Response:**
```json
{
  "success": true,
  "errors": "",
  "warnings": ""
}
```

## Pinout Verification

### `find_datasheet`

Resolve and optionally download the official Microchip datasheet PDF for a device.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `partNumber` | string | yes | Device part number |
| `download` | bool | no | If true, download and cache the PDF locally (default: false) |

**Response:**
```json
{
  "url": "https://ww1.microchip.com/downloads/...",
  "cached": true,
  "localPath": "/path/to/dfp_cache/datasheets/DSPIC33CK64MP102.pdf"
}
```

### `verify_pinout`

Send a datasheet PDF to an LLM (Anthropic or OpenAI) to cross-check the parsed pinout against the official documentation and extract CLC input source mappings.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `pdfBase64` | string | yes | Base64-encoded PDF file |
| `partNumber` | string | yes | Device part number |
| `package` | string | no | Package variant to verify |
| `apiKey` | string | no | LLM API key (falls back to `ANTHROPIC_API_KEY` or `OPENAI_API_KEY` env var) |
| `provider` | string | no | LLM provider: `"anthropic"` (default) or `"openai"` |

**Response:** `VerifyResult` with per-package pin corrections, match scores, notes, and `clc_input_sources` (extracted CLC input source mapping, if found in the datasheet).

### `apply_overlay`

Save verified pinout corrections as a JSON overlay file in `pinouts/`. If the verification result included CLC input source mappings, they are also saved to `clc_sources/`.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `partNumber` | string | yes | Device part number |
| `packages` | object | yes | Package name -> pin data mapping |

**Response:**
```json
{
  "success": true,
  "path": "pinouts/DSPIC33CK64MP102.json"
}
```

### `api_key_status`

Check which LLM API keys are configured (via `.env` or environment variables). Supports both Anthropic (Anthropic) and OpenAI providers.

**Parameters:** none

**Response:**
```json
{
  "anthropic": { "configured": true, "hint": "...sk-1234" },
  "openai": { "configured": true, "hint": "...sk-5678" }
}
```
