# Tauri Commands

pickle exposes its desktop/backend API through `#[tauri::command]` handlers in `src-tauri/src/commands.rs` and `src-tauri/src/commands/*.rs`.

- Frontend calls use `window.__TAURI__.core.invoke("command_name", { ...args })`.
- Some long-running verification commands also emit `verify-progress` events so the UI can stream status text.
- Request field names passed from JS are camelCase; several response payloads are hand-built JSON and therefore intentionally use the field names shown below.

## Catalog

| Command | Purpose |
|---|---|
| `list_devices` | List cached and indexed part numbers |
| `refresh_index` | Refresh the Microchip pack index cache |
| `index_status` | Report pack index availability/staleness |
| `load_app_settings` | Load or create `settings.toml` |
| `set_theme_mode` | Persist theme preference |
| `remember_last_used_device` | Persist last loaded part/package |
| `load_device` | Resolve a device/package into the frontend payload |
| `open_text_file_dialog` | Pick and read a text file |
| `open_binary_file_dialog` | Pick and read a binary file as base64 |
| `save_text_file_dialog` | Save a text file via native dialog |
| `export_generated_files_dialog` | Export multiple generated files into a folder |
| `generate_code` | Generate the configured `<basename>.c` and `<basename>.h` pair |
| `compiler_info` | Resolve the active family compiler and report version/path |
| `compile_check` | Compile-check generated code with the resolved PIC24/dsPIC33 compiler |
| `find_datasheet` | Resolve a datasheet from cache/download/text fallback |
| `verify_pinout` | Run datasheet verification with OpenAI or Anthropic |
| `apply_overlay` | Save package pin overlays to `pinouts/` |
| `api_key_status` | Report whether any supported verification key is configured |

## Device Index And Loading

### `list_devices`

Returns known part numbers from the pack index plus the subset already cached locally.

Request: none

Response:

```json
{
  "devices": ["DSPIC33CK64MP102", "DSPIC33CK256MP508"],
  "cached": ["DSPIC33CK64MP102"],
  "total": 347,
  "cached_count": 1
}
```

### `refresh_index`

Fetches the latest Microchip pack index and refreshes the local cache.

Request: none

Response:

```json
{
  "success": true,
  "device_count": 347,
  "pack_count": 42,
  "age_hours": 0.0
}
```

### `index_status`

Reads the current pack-index cache without refreshing it.

Request: none

Response:

```json
{
  "available": true,
  "device_count": 347,
  "pack_count": 42,
  "age_hours": 12.5,
  "is_stale": false
}
```

### `load_device`

Loads a device by part number, picks the requested package when valid, resolves pins, and returns the full frontend payload.

Request:

| Field | Type | Required | Notes |
|---|---|---|---|
| `partNumber` | string | yes | Canonical or user-entered part number |
| `package` | string | no | Package override; falls back to the device default pinout |

Response:

```json
{
  "part_number": "DSPIC33CK64MP102",
  "selected_package": "SSOP-28",
  "packages": {
    "SSOP-28": { "pin_count": 28, "source": "edc" }
  },
  "pin_count": 28,
  "pins": [],
  "remappable_inputs": [],
  "remappable_outputs": [],
  "pps_input_mappings": [],
  "pps_output_mappings": [],
  "port_registers": {},
  "fuse_defs": [],
  "clc_input_sources": null
}
```

`pins` contains resolved per-position pin objects from `DeviceData::resolve_pins()`, including pad name, functions, RP number, port metadata, and package position.

## Settings

### `load_app_settings`

Loads and normalizes `settings.toml`, creating it with defaults if missing or empty.

Request: none

Response:

```json
{
  "path": "/Users/me/Library/Application Support/pickle/settings.toml",
  "settings": {
    "appearance": { "theme": "dark" },
    "startup": { "device": "last-used", "package": "" },
    "toolchain": {
      "fallback_compiler": "xc-dsc-gcc",
      "family_compilers": {
        "pic24": "xc16-gcc",
        "dspic33": "xc-dsc-gcc"
      }
    },
    "codegen": {
      "output_basename": "mcu_init"
    },
    "last_used": { "part_number": "DSPIC33CK64MP102", "package": "SSOP-28" }
  }
}
```

### `set_theme_mode`

Persists the selected theme.

Request:

| Field | Type | Required |
|---|---|---|
| `theme` | string | yes |

Allowed values are `dark`, `light`, and `system`. Invalid values are normalized by the backend.

### `remember_last_used_device`

Stores the most recently loaded part/package so startup policy `last-used` can reopen it later.

Request:

| Field | Type | Required |
|---|---|---|
| `partNumber` | string | yes |
| `package` | string | no |

Response: none

## Native File Dialogs

All dialog commands return `null` when the user cancels.

### `open_text_file_dialog`

Request:

```json
{
  "request": {
    "title": "Open Pin Configuration",
    "filters": [{ "name": "JSON", "extensions": ["json"] }]
  }
}
```

Response:

```json
{
  "path": "/Users/me/config.json",
  "contents": "{\n  \"part_number\": \"DSPIC33CK64MP102\"\n}"
}
```

### `open_binary_file_dialog`

Same request shape as `open_text_file_dialog`.

Response:

```json
{
  "path": "/Users/me/datasheet.pdf",
  "name": "datasheet.pdf",
  "base64": "JVBERi0xLjcK..."
}
```

### `save_text_file_dialog`

Request:

```json
{
  "request": {
    "title": "Save Pin Configuration",
    "suggestedName": "DSPIC33CK64MP102_SSOP-28.json",
    "contents": "{...}",
    "filters": [{ "name": "JSON", "extensions": ["json"] }]
  }
}
```

Response:

```json
{
  "path": "/Users/me/DSPIC33CK64MP102_SSOP-28.json"
}
```

### `export_generated_files_dialog`

Request:

```json
{
  "request": {
    "title": "Export Generated C Files",
    "files": {
      "mcu_init.c": "/* ... */",
      "mcu_init.h": "/* ... */"
    }
  }
}
```

Response:

```json
{
  "directory": "/Users/me/Desktop/pickle-export",
  "writtenFiles": [
    "/Users/me/Desktop/pickle-export/mcu_init.c",
    "/Users/me/Desktop/pickle-export/mcu_init.h"
  ]
}
```

## Code Generation And Compile Checks

### `generate_code`

Generates the source/header pair used by the code tab and export flow. The
default basename is `mcu_init`, but the backend reads `[codegen].output_basename`
from `settings.toml` and names both files from that value.

Request:

| Field | Type | Required | Notes |
|---|---|---|---|
| `request.partNumber` | string | yes | Target part number |
| `request.package` | string | no | Package variant |
| `request.assignments` | array | yes | Flattened assignment list |
| `request.signalNames` | object | no | Pin-position keyed alias map |
| `request.digitalPins` | array | no | Explicit digital overrides |
| `request.oscillator` | object | no | `source`, `targetFoscMhz`, `crystalMhz`, `poscmd` |
| `request.fuses` | object | no | `selections` nested by register and field |
| `request.clc` | object | no | Module-number keyed `ClcModuleConfig` map |

If `request.oscillator` is present, clock-related fuse fields that it owns are
still accepted in `request.fuses`, but they are ignored during emission so the
generated source cannot contain duplicate `#pragma config` lines.

Assignment entry shape:

```json
{
  "pinPosition": 5,
  "rpNumber": 36,
  "peripheral": "U1TX",
  "direction": "out",
  "ppsval": 1,
  "fixed": false
}
```

Response:

```json
{
  "files": {
    "mcu_init.c": "/* generated source */",
    "mcu_init.h": "/* generated header */"
  }
}
```

### `compiler_info`

Resolves the compiler for the requested part family, then checks for that executable on `PATH` or common Microchip install roots. When a `partNumber` is provided, availability also requires a matching compiler DFP (`-mdfp`) for that device.

Request:

| Field | Type | Required | Notes |
|---|---|---|---|
| `partNumber` | string | no | Optional part number used to choose `xc16-gcc` for PIC24 or `xc-dsc-gcc` for dsPIC33 |

Response:

```json
{
  "available": true,
  "command": "xc-dsc-gcc",
  "device_family": "dspic33",
  "path": "/Applications/microchip/xc-dsc/v3.10/bin/xc-dsc-gcc",
  "version": "Microchip MPLAB XC-DSC C Compiler v3.10"
}
```

### `compile_check`

Writes the generated files into a temporary directory and invokes the configured family compiler with `-mdfp=<pack>/xc16` and `-mcpu=<part>`. The DFP is resolved from installed MPLAB X packs first, then from pickle's cached/downloaded `.atpack` files.

Request:

| Field | Type | Required |
|---|---|---|
| `request.code` | string | yes |
| `request.header` | string | no |
| `request.partNumber` | string | yes |

Response:

```json
{
  "success": true,
  "command": "xc16-gcc",
  "device_family": "pic24",
  "errors": "",
  "warnings": ""
}
```

## Datasheet Lookup And Verification

### `find_datasheet`

Search order:

1. cached datasheet files under `dfp_cache/datasheets`
2. shallow scan of `~/Downloads`
3. Microchip product-page resolution and PDF download
4. text-extraction fallback when PDF download fails

Request:

| Field | Type | Required |
|---|---|---|
| `partNumber` | string | yes |

Response is `null` when nothing can be resolved. Otherwise it is one of these shapes:

Local or downloaded PDF:

```json
{
  "name": "DSPIC33CK64MP102-DS70005349.pdf",
  "base64": "JVBERi0xLjcK...",
  "source": "local"
}
```

The local variant also includes `path`; the downloaded variant also includes `revision`.

Text fallback:

```json
{
  "name": "DSPIC33CK64MP102-DS70005349.md",
  "text": "# Extracted datasheet text...",
  "source": "text_proxy",
  "revision": "DS70005349",
  "pdf_url": "https://..."
}
```

This command emits `verify-progress` events while it searches/downloads.

### `verify_pinout`

Runs the LLM-backed datasheet comparison.

Request:

| Field | Type | Required | Notes |
|---|---|---|---|
| `pdfBase64` | string | yes | Datasheet bytes encoded as base64 |
| `partNumber` | string | yes | Device under test |
| `package` | string | no | Package to compare against |
| `apiKey` | string | no | Optional override; otherwise backend checks `OPENAI_API_KEY` first, then `ANTHROPIC_API_KEY` |

Response: serialized `VerifyResult`

```json
{
  "part_number": "DSPIC33CK64MP102",
  "packages": {},
  "notes": [],
  "clc_input_sources": [
    ["CLCINA", "Fcy", "CLC3OUT", "LPRC", "REFCLKO", "Reserved", "SCCP2 Aux", "SCCP4 Aux"],
    ["CLCINB", "Reserved", "CMP1", "UART1 TX", "Reserved", "Reserved", "SCCP1 OC", "SCCP2 OC"],
    ["CLCINC", "CLC1OUT", "CMP2", "SPI1 SDO", "UART1 RX", "CLC4OUT", "SCCP3 CEF", "SCCP4 CEF"],
    ["PWM Event A", "CLC2OUT", "CMP3", "SPI1 SDI", "Reserved", "CLCIND", "SCCP1 Aux", "SCCP3 Aux"]
  ]
}
```

The real response includes per-package pin maps, `pin_functions`, correction lists, and optional extracted CLC input-source tables. This command also emits `verify-progress` events.

### `apply_overlay`

Writes package pin overlays to `pinouts/<PART>.json`.

Request:

```json
{
  "request": {
    "partNumber": "DSPIC33CK64MP102",
    "packages": {
      "SSOP-28": {
        "pin_count": 28,
        "pins": {
          "1": "MCLR",
          "2": "RB0"
        }
      }
    }
  }
}
```

Response:

```json
{
  "success": true,
  "path": "/.../pinouts/DSPIC33CK64MP102.json"
}
```

### `api_key_status`

Reports whether either supported provider key is available.

Request: none

Response:

```json
{
  "configured": true,
  "hint": "...abcd"
}
```
