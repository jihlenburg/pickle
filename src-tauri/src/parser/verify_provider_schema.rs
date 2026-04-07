//! Shared structured-output schema helpers for verification providers.
//!
//! OpenAI and Anthropic use different transport APIs, but both are asked to
//! produce the same logical verification shape. Keeping that schema here avoids
//! duplicating the contract across provider-specific modules.

use serde_json::Value;

pub(crate) fn verification_output_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "packages": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "package_name": { "type": "string" },
                        "pin_count": { "type": "integer" },
                        "pins": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "pin_number": { "type": "integer" },
                                    "pad_name": { "type": "string" }
                                },
                                "required": ["pin_number", "pad_name"],
                                "additionalProperties": false
                            }
                        },
                        "pin_functions": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "pad_name": { "type": "string" },
                                    "functions": {
                                        "type": "array",
                                        "items": { "type": "string" }
                                    }
                                },
                                "required": ["pad_name", "functions"],
                                "additionalProperties": false
                            }
                        }
                    },
                    "required": ["package_name", "pin_count", "pins", "pin_functions"],
                    "additionalProperties": false
                }
            },
            "corrections": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "pin_position": { "type": "integer" },
                        "package": { "type": "string" },
                        "current_pad": { "type": "string" },
                        "datasheet_pad": { "type": "string" },
                        "type": { "type": "string" },
                        "note": { "type": "string" }
                    },
                    "required": ["pin_position", "package", "current_pad", "datasheet_pad", "type", "note"],
                    "additionalProperties": false
                }
            },
            "clc_input_sources": {
                "type": "array",
                "items": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "notes": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        "required": ["packages", "corrections", "clc_input_sources", "notes"],
        "additionalProperties": false
    })
}

pub(crate) fn anthropic_verification_tool() -> Value {
    serde_json::json!({
        "name": "submit_verification",
        "description": "Return the extracted package pin tables, discrepancy list, optional CLC input sources, and notes as structured verification data.",
        "strict": true,
        "input_schema": verification_output_schema()
    })
}

pub(crate) fn openai_verification_text_format() -> Value {
    serde_json::json!({
        "format": {
            "type": "json_schema",
            "name": "pinout_verification",
            "schema": verification_output_schema(),
            "strict": true
        }
    })
}

pub(crate) fn normalize_structured_verification_output(input: &Value) -> Value {
    let mut normalized = serde_json::Map::new();

    let mut packages_out = serde_json::Map::new();
    if let Some(packages) = input.get("packages").and_then(|value| value.as_array()) {
        for package in packages {
            let Some(package_name) = package.get("package_name").and_then(|value| value.as_str())
            else {
                continue;
            };

            let pin_count = package
                .get("pin_count")
                .cloned()
                .unwrap_or_else(|| Value::Number(0u64.into()));

            let mut pins = serde_json::Map::new();
            if let Some(pin_items) = package.get("pins").and_then(|value| value.as_array()) {
                for pin in pin_items {
                    let Some(pin_number) = pin.get("pin_number").and_then(|value| value.as_u64())
                    else {
                        continue;
                    };
                    let Some(pad_name) = pin.get("pad_name").and_then(|value| value.as_str())
                    else {
                        continue;
                    };
                    pins.insert(pin_number.to_string(), Value::String(pad_name.to_string()));
                }
            }

            let mut pin_functions = serde_json::Map::new();
            if let Some(function_items) = package
                .get("pin_functions")
                .and_then(|value| value.as_array())
            {
                for function_item in function_items {
                    let Some(pad_name) = function_item
                        .get("pad_name")
                        .and_then(|value| value.as_str())
                    else {
                        continue;
                    };
                    let functions = function_item
                        .get("functions")
                        .and_then(|value| value.as_array())
                        .cloned()
                        .unwrap_or_default();
                    pin_functions.insert(pad_name.to_string(), Value::Array(functions));
                }
            }

            packages_out.insert(
                package_name.to_string(),
                Value::Object(
                    [
                        ("pin_count".to_string(), pin_count),
                        ("pins".to_string(), Value::Object(pins)),
                        ("pin_functions".to_string(), Value::Object(pin_functions)),
                    ]
                    .into_iter()
                    .collect(),
                ),
            );
        }
    }
    normalized.insert("packages".to_string(), Value::Object(packages_out));
    normalized.insert(
        "corrections".to_string(),
        input
            .get("corrections")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
    );
    normalized.insert(
        "notes".to_string(),
        input
            .get("notes")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
    );
    normalized.insert(
        "clc_input_sources".to_string(),
        input
            .get("clc_input_sources")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new())),
    );

    Value::Object(normalized)
}
