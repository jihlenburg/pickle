# Tauri Commands

pickle exposes its desktop/backend API through `#[tauri::command]` handlers under `src-tauri/src/commands/`, with `src-tauri/src/commands.rs` acting as the command facade/re-export root.

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
| `set_welcome_intro_seen` | Persist first-launch intro dismissal state |
| `remember_last_used_device` | Persist last loaded part/package |
| `load_device` | Resolve a device/package into the frontend payload |
| `open_text_file_dialog` | Pick and read a text file |
| `open_binary_file_dialog` | Pick and read a binary file as base64 |
| `save_text_file_dialog` | Save a text file via native dialog |
| `write_text_file_path` | Write text directly to a known path without opening a dialog |
| `delete_file_path` | Delete a previously saved file path |
| `export_generated_files_dialog` | Export multiple generated files into a folder |
| `generate_code` | Generate the configured `<basename>.c` and `<basename>.h` pair |
| `compiler_info` | Resolve the active family compiler and report version/path |
| `compile_check` | Compile-check generated code with the resolved PIC24/dsPIC33 compiler |
| `find_datasheet` | Resolve a datasheet from cache/download/text fallback |
| `verify_pinout` | Run datasheet verification with OpenAI or Anthropic |
| `apply_overlay` | Save package pin overlays to `pinouts/` |
| `set_package_display_name` | Save or clear a local display-name override for any package |
| `rename_overlay_package` | Rename an overlay-backed package entry |
| `delete_overlay_package` | Delete an overlay-backed package entry |
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
    "SSOP-28": { "pin_count": 28, "source": "edc", "display_name": null }
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

`selected_package` and the keys in `packages` are the stored backend package identifiers. The frontend may normalize some internal EDC-coded labels such as `STX04 (48-pin uQFN)` into a friendlier display label like `48-PIN VQFN`, and package metadata can also carry an explicit `display_name` override. In both cases, the backend keys remain unchanged for lookups, config files, and overlay merges. The current package selector stays visible in the header even when only one visible package exists; rename/reset/delete actions live in the attached package-actions menu rather than altering the backend key directly.

`has_clc` is derived from the parsed device data, not only from a cached `clc_module_id`. That means devices with visible `CLCINx` / `CLCxOUT` endpoints still expose the CLC tab even if an older cache predates the newer module-ID parsing pass.

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
    "verification": {
      "provider": "auto"
    },
    "onboarding": {
      "welcome_intro_seen": false
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

### `set_welcome_intro_seen`

Persists whether the first-launch intro overlay has already been dismissed.

Request:

| Field | Type | Required |
|---|---|---|
| `seen` | bool | yes |

Response: none

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

The frontend uses these commands in two different ways:

- first-save / save-as / rename flows go through `save_text_file_dialog`
- direct save to an existing config path uses `write_text_file_path`

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

### `write_text_file_path`

Writes text directly to a known path. This is what the normal `Save` action
uses once a config file already has a path.

Request:

```json
{
  "path": "/Users/me/DSPIC33CK64MP102_SSOP-28.json",
  "contents": "{...}"
}
```

Response:

```json
{
  "path": "/Users/me/DSPIC33CK64MP102_SSOP-28.json"
}
```

### `delete_file_path`

Deletes a file path if it exists. The current frontend rename flow writes the
new file first, then removes the previous path with this command.

Request:

```json
{
  "path": "/Users/me/old-name.json"
}
```

Response: none

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

Config-file JSON written by the frontend save/load flow currently contains:

```json
{
  "part_number": "DSPIC33CK64MP102",
  "package": "SSOP-28",
  "assignments": {},
  "signal_names": {},
  "reserved_assignments": {
    "jtag": {},
    "i2c": {}
  },
  "oscillator": null,
  "fuses": { "selections": {} },
  "clc": null
}
```

## Code Generation And Compile Checks

### `generate_code`

Generates the source/header pair used by the code tab and export flow. The
default basename is `mcu_init`, but the backend reads `[codegen].output_basename`
from `settings.toml` and names both files from that value. The frontend save,
export, and compile-check flows all consume those generated filenames instead of
assuming a hard-coded basename.

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

For dsPIC33AK parts, the backend now synthesizes the shared runtime
clock-generator register sequence automatically. The generated source uses
`OSCCFG`, `CLK1CON`, `CLK1DIV`, `PLL1CON`, and `PLL1DIV` instead of CK-style
`FNOSC` / `POSCMD` / `PLLKEN` pragmas, and still strips those legacy fuse
fields from incoming fuse blocks to avoid conflicting output.

See [CLC](clc.md) for the exact `ClcModuleConfig` field layout and the way the
frontend normalizes/persists configured modules.

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

When the generated source includes `#include "<basename>.h"`, the backend uses
that include to choose the temporary source/header filenames. That keeps
compile-check aligned with custom `[codegen].output_basename` values.

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
   Local candidates are re-validated against the selected part before reuse. pickle accepts an exact part match or a sibling-family match with the same family-series marker such as `MC10` for `MC105` / `MC106`, and ignores obviously clipped PDFs that are too small to be a full datasheet. After `find_datasheet` resolves a Microchip family PDF for a specific part, later validation also trusts the cached datasheet number/title from that resolution, so a valid family PDF can be reused even when its title does not repeat the exact selected part suffix.
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

The real response includes per-package pin maps, `pin_functions`, locally derived correction lists, and optional extracted CLC input-source tables. Package tables are filtered by both pin count and explicit device-branch compatibility when a family datasheet mixes variants such as `MC` and `MPS`. The backend now treats the provider pass as datasheet-table extraction only and computes mismatches against the loaded device locally, which keeps corrections deterministic and lets sibling parts sharing the same family PDF reuse the same cached extraction result. This command also emits `verify-progress` events with staged payloads such as `datasheet.search`, `provider.upload`, and `result.done`, plus optional `detail`, `progress`, and `provider` fields.

Implementation note: the command layer is now split into `commands/verification/lookup.rs` for datasheet discovery, `commands/verification/run.rs` for the provider-backed pinout/CLC commands, `commands/verification/overlay.rs` for overlay/name/status actions, and `commands/verification_support.rs` for shared datasheet decode/cache/device-context prep. The parser-side verification path is split into prompt/cache-scope helpers (`verify_prompt.rs`), shared progress payloads (`verify_progress.rs`), bookmark/PDF reduction and PNG rendering (`verify_pdf.rs`), provider dispatch (`verify_provider.rs`), shared provider schema (`verify_provider_schema.rs`), Anthropic/OpenAI transports (`verify_provider_anthropic.rs`, `verify_provider_openai.rs`), OpenAI stream normalization (`verify_openai_stream.rs`), local comparison/filtering (`verify_compare.rs`), overlay persistence (`verify_overlay.rs`), and the cache-aware runner (`pinout_verifier.rs`).

The non-LLM DFP/data-source side is also split more explicitly now: `dfp_paths.rs` owns read-root and cache-path policy, `dfp_datasheet.rs` owns PDF probing/validation plus local datasheet cache reuse, `dfp_store.rs` owns cached-device JSON plus overlay/CLC-source persistence, and `dfp_manager.rs` stays focused on device-pack lookup/extraction and compiler-support orchestration.

### `apply_overlay`

Writes package pin overlays to `pinouts/<PART>.json`. If the verified package
table is pin-for-pin identical to an existing device-pack variant, the command
now stores a display-name override on that canonical package key instead of
creating a second redundant overlay package entry.

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
  "path": "/.../pinouts/DSPIC33CK64MP102.json",
  "packageName": "SSOP-28"
}
```

### `set_package_display_name`

Saves or clears a local display-name override for any package inside `pinouts/<PART>.json`. This changes only the UI label. The stored backend package key remains unchanged.

Request:

```json
{
  "request": {
    "partNumber": "DSPIC33CK64MP102",
    "packageName": "STX04 (48-pin uQFN)",
    "displayName": "48-PIN VQFN"
  }
}
```

To clear the override and fall back to the default label, send `null` for `displayName`.

Response:

```json
{
  "success": true,
  "path": "/.../pinouts/DSPIC33CK64MP102.json",
  "packageName": "STX04 (48-pin uQFN)",
  "displayName": "48-PIN VQFN"
}
```

### `rename_overlay_package`

Renames an existing overlay-backed package key inside `pinouts/<PART>.json`. Built-in device-pack package entries are not modified by this command. This is distinct from `set_package_display_name`, which only changes the UI label.

Request:

```json
{
  "request": {
    "partNumber": "DSPIC33CK64MP102",
    "oldPackageName": "48-PIN TQFP",
    "newPackageName": "48-PIN TQFP (7x7)"
  }
}
```

Response:

```json
{
  "success": true,
  "path": "/.../pinouts/DSPIC33CK64MP102.json",
  "packageName": "48-PIN TQFP (7x7)"
}
```

### `delete_overlay_package`

Deletes an overlay-backed package entry from `pinouts/<PART>.json`. If that removal leaves the overlay file empty, the backend deletes the file.

Request:

```json
{
  "request": {
    "partNumber": "DSPIC33CK64MP102",
    "packageName": "48-PIN TQFP"
  }
}
```

Response:

```json
{
  "success": true,
  "path": null,
  "packageName": "48-PIN TQFP"
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
