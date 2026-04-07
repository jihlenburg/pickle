//! Verification runner for datasheet-based pinout and CLC extraction.
//!
//! Prompt building, provider HTTP transport, PDF/page reduction, and local
//! comparison now live in sibling modules. This file keeps the orchestration
//! boundary: choose the provider, reuse cached extraction results, invoke the
//! transport, and hand the structured response to the local compare pass.

use std::collections::HashMap;

use serde_json::Value;

use crate::parser::verifier_cache::{
    load_cached_verify, save_cached_verify, verify_cache_disabled,
};
use crate::parser::verify_compare::{
    build_verify_result, parse_verifier_extraction, parse_verifier_response,
    verifier_extraction_from_value, VerifyResult,
};
use crate::parser::verify_progress::emit_progress;
pub use crate::parser::verify_progress::{ProgressCallback, VerifyProgressUpdate};
use crate::parser::verify_prompt::{
    build_task_prompt, provider_analysis_hint, provider_name, task_label, verification_cache_scope,
    VerifyTask,
};
pub use crate::parser::verify_provider::get_api_key;
use crate::parser::verify_provider::{call_llm_api, resolve_provider};

fn verify_with_task(
    task: VerifyTask,
    pdf_bytes: &[u8],
    datasheet_text: Option<&str>,
    device_data: &Value,
    api_key: Option<&str>,
    progress: Option<&ProgressCallback>,
) -> Result<VerifyResult, String> {
    let (provider, key) = resolve_provider(api_key)?;
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "provider.select",
            0.36,
            format!(
                "Using {} for {} verification",
                provider_name(provider),
                task_label(task)
            ),
        )
        .detail(provider_analysis_hint(provider, task))
        .provider(provider),
    );

    let prompt = build_task_prompt(provider, task, device_data);
    let cache_scope = verification_cache_scope(provider, task, device_data);

    if !pdf_bytes.is_empty() && !verify_cache_disabled() {
        if let Some(cached_json) = load_cached_verify(pdf_bytes, &cache_scope) {
            log::info!("verify_pinout: using cached LLM result");
            emit_progress(
                progress,
                VerifyProgressUpdate::new("result.cached", 0.95, "Using cached verification result")
                    .detail("This datasheet was already verified earlier for the same provider and request scope, so no provider call was needed."),
            );
            let cached_raw = serde_json::to_string(&cached_json).unwrap_or_default();
            return Ok(build_verify_result(
                &verifier_extraction_from_value(&cached_json),
                device_data,
                &cached_raw,
            ));
        }
    } else if !pdf_bytes.is_empty() && verify_cache_disabled() {
        log::info!("verify_pinout: verify cache disabled via PICKLE_DISABLE_VERIFY_CACHE");
    }

    let part_number = device_data
        .get("part_number")
        .and_then(|value| value.as_str())
        .unwrap_or("UNKNOWN");

    let raw_response = call_llm_api(
        provider,
        pdf_bytes,
        datasheet_text,
        task,
        &prompt,
        &key,
        part_number,
        progress,
    )?;
    emit_progress(
        progress,
        VerifyProgressUpdate::new(
            "result.process",
            0.94,
            "Processing the structured verification result",
        )
        .detail("Comparing extracted package data against the loaded device and preparing the overlay view."),
    );

    if !pdf_bytes.is_empty() {
        if let Ok(extraction) = parse_verifier_extraction(&raw_response) {
            if task == VerifyTask::Pinout && extraction.packages.is_empty() {
                log::warn!(
                    "verify_pinout: skipping cache write because the structured extraction returned zero packages"
                );
            } else if let Ok(cached_json) = serde_json::to_value(&extraction) {
                save_cached_verify(pdf_bytes, &cache_scope, &cached_json);
            }
            return Ok(build_verify_result(&extraction, device_data, &raw_response));
        }
    }

    Ok(parse_verifier_response(&raw_response, device_data))
}

pub fn verify_pinout(
    pdf_bytes: &[u8],
    datasheet_text: Option<&str>,
    device_data: &Value,
    api_key: Option<&str>,
    progress: Option<&ProgressCallback>,
) -> Result<VerifyResult, String> {
    verify_with_task(
        VerifyTask::Pinout,
        pdf_bytes,
        datasheet_text,
        device_data,
        api_key,
        progress,
    )
}

pub fn verify_clc(
    pdf_bytes: &[u8],
    datasheet_text: Option<&str>,
    device_data: &Value,
    api_key: Option<&str>,
    progress: Option<&ProgressCallback>,
) -> Result<VerifyResult, String> {
    match verify_with_task(
        VerifyTask::Clc,
        pdf_bytes,
        datasheet_text,
        device_data,
        api_key,
        progress,
    ) {
        Ok(result) => Ok(result),
        Err(error) if error.contains("No bookmark or text ranges found for the CLC section") => {
            Ok(VerifyResult {
                part_number: device_data
                    .get("part_number")
                    .and_then(|value| value.as_str())
                    .unwrap_or("UNKNOWN")
                    .to_string(),
                packages: HashMap::new(),
                notes: vec![
                    "No CLC section could be located in this datasheet by bookmark or page-text scan, so no background CLC extraction was run."
                        .to_string(),
                ],
                clc_input_sources: None,
                raw_response: String::new(),
            })
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::verify_compare::{
        package_matches_selected_device_branch, parse_verifier_extraction, parse_verifier_response,
        verifier_extraction_from_value,
    };
    use crate::parser::verify_openai_stream::{
        extract_openai_text, normalize_openai_output_text, parse_openai_stream_reader,
    };
    use crate::parser::verify_overlay::{
        delete_overlay_package_from_json, rename_overlay_package_in_json,
        set_package_display_name_in_json,
    };
    use crate::parser::verify_pdf::{
        collect_pdf_bookmarks, select_clc_page_spans, select_clc_page_spans_from_text_hits,
        select_pinout_page_spans, select_pinout_page_spans_from_text_hits, BookmarkEntry, PageSpan,
    };
    use crate::parser::verify_prompt::{verification_cache_scope, Provider, VerifyTask};
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Bookmark, Document, Object, Stream};
    use pdfium_auto::bind_pdfium_silent;
    use serde_json::{json, Value};

    fn bookmark(title: &str, page: u32, depth: usize) -> BookmarkEntry {
        BookmarkEntry {
            title: title.to_string(),
            page,
            depth,
        }
    }

    fn build_action_bookmark_pdf() -> Vec<u8> {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
        });
        let resources_id = doc.add_object(dictionary! {
            "Font" => dictionary! {
                "F1" => font_id,
            },
        });

        let mut page_ids = Vec::new();

        for page_number in 1..=2 {
            let content = Content {
                operations: vec![
                    Operation::new("BT", vec![]),
                    Operation::new("Tf", vec!["F1".into(), 24.into()]),
                    Operation::new("Td", vec![72.into(), 720.into()]),
                    Operation::new(
                        "Tj",
                        vec![Object::string_literal(format!(
                            "Bookmark page {page_number}"
                        ))],
                    ),
                    Operation::new("ET", vec![]),
                ],
            };
            let content_id = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));
            let page_id = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "Contents" => content_id,
                "Resources" => resources_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            });
            page_ids.push(page_id);
        }

        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids
                    .iter()
                    .copied()
                    .map(Object::Reference)
                    .collect::<Vec<_>>(),
                "Count" => page_ids.len() as i64,
            }),
        );

        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "PageMode" => "UseOutlines",
        });

        doc.add_bookmark(
            Bookmark::new("Pin Diagrams".to_string(), [0.0, 0.0, 1.0], 0, page_ids[0]),
            None,
        );
        doc.add_bookmark(
            Bookmark::new(
                "Pinout I/O Descriptions".to_string(),
                [0.0, 0.0, 1.0],
                0,
                page_ids[1],
            ),
            None,
        );

        if let Some(outline_id) = doc.build_outline() {
            if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(catalog_id) {
                dict.set("Outlines", Object::Reference(outline_id));
            }
        }

        doc.trailer.set("Root", catalog_id);

        let mut pdf_bytes = Vec::new();
        doc.save_to(&mut pdf_bytes).unwrap();
        pdf_bytes
    }

    #[test]
    fn pinout_bookmark_ranges_keep_pin_sections_through_table_of_contents() {
        let bookmarks = vec![
            bookmark("dsPIC33AK128MC106 Product Family", 7, 0),
            bookmark("Pin Diagrams", 9, 0),
            bookmark("Pinout I/O Descriptions", 16, 0),
            bookmark("Terminology Cross Reference", 21, 0),
            bookmark("Table of Contents", 22, 0),
            bookmark("1. Device Overview", 28, 0),
            bookmark("Configurable Logic Cell (CLC)", 1292, 0),
            bookmark("Peripheral Trigger Generator (PTG)", 1311, 0),
        ];

        let spans = select_pinout_page_spans(&bookmarks, 1528);

        assert_eq!(spans, vec![PageSpan { start: 9, end: 21 }]);
    }

    #[test]
    fn clc_bookmark_ranges_keep_only_clc_chapter() {
        let bookmarks = vec![
            bookmark("dsPIC33AK128MC106 Product Family", 7, 0),
            bookmark("Pin Diagrams", 9, 0),
            bookmark("Pinout I/O Descriptions", 16, 0),
            bookmark("Table of Contents", 22, 0),
            bookmark("1. Device Overview", 28, 0),
            bookmark("Configurable Logic Cell (CLC)", 1292, 0),
            bookmark("Peripheral Trigger Generator (PTG)", 1311, 0),
        ];

        let spans = select_clc_page_spans(&bookmarks, 1528);

        assert_eq!(
            spans,
            vec![PageSpan {
                start: 1292,
                end: 1310
            }]
        );
    }

    #[test]
    fn clc_text_hits_expand_to_a_small_contiguous_span() {
        let spans = select_clc_page_spans_from_text_hits(&[198, 199, 201, 203, 204], 260);

        assert_eq!(
            spans,
            vec![PageSpan {
                start: 197,
                end: 205
            }]
        );
    }

    #[test]
    fn pinout_text_hits_expand_until_table_of_contents() {
        let spans = select_pinout_page_spans_from_text_hits(&[9, 16], &[22], 300);

        assert_eq!(spans, vec![PageSpan { start: 9, end: 21 }]);
    }

    #[test]
    fn pinout_text_hits_without_table_of_contents_cover_small_trimmed_pdf() {
        let spans = select_pinout_page_spans_from_text_hits(&[1, 8], &[], 12);

        assert_eq!(spans, vec![PageSpan { start: 1, end: 12 }]);
    }

    #[test]
    fn bookmark_ranges_fall_back_to_first_numbered_chapter_when_toc_is_missing() {
        let bookmarks = vec![
            bookmark("Pin Diagrams", 9, 0),
            bookmark("Pinout I/O Descriptions", 16, 0),
            bookmark("1. Device Overview", 28, 0),
        ];

        let spans = select_pinout_page_spans(&bookmarks, 300);

        assert_eq!(spans, vec![PageSpan { start: 9, end: 27 }]);
    }

    #[test]
    fn nested_bookmark_ranges_include_pinout_descriptions_after_pin_diagrams() {
        let bookmarks = vec![
            bookmark(
                "dsPIC33EPXXXGP50X, dsPIC33EPXXXMC20X/50X and PIC24EPXXXGP/MC20X Product Families",
                2,
                0,
            ),
            bookmark("Pin Diagrams", 5, 1),
            bookmark("Pin Diagrams (Continued)", 6, 1),
            bookmark("Table of Contents", 24, 1),
            bookmark("1.0 Device Overview", 27, 0),
            bookmark("TABLE 1-1: Pinout I/O Descriptions", 28, 1),
            bookmark(
                "2.0 Guidelines for Getting Started with 16-Bit Digital Signal Controllers and Microcontrollers",
                31,
                0,
            ),
        ];

        let spans = select_pinout_page_spans(&bookmarks, 546);

        assert_eq!(
            spans,
            vec![
                PageSpan { start: 5, end: 23 },
                PageSpan { start: 28, end: 30 }
            ]
        );
    }

    #[test]
    fn nested_clc_bookmark_ends_at_next_sibling_or_parent_section() {
        let bookmarks = vec![
            bookmark("Specialized Peripherals", 190, 0),
            bookmark("Configurable Logic Cell (CLC)", 200, 1),
            bookmark("Register 1-1: CLC1CON", 201, 2),
            bookmark("Comparator", 220, 1),
        ];

        let spans = select_clc_page_spans(&bookmarks, 260);

        assert_eq!(
            spans,
            vec![PageSpan {
                start: 200,
                end: 219
            }]
        );
    }

    #[test]
    fn collect_pdf_bookmarks_resolves_action_backed_outline_targets() {
        if bind_pdfium_silent().is_err() {
            return;
        }

        let pdf_bytes = build_action_bookmark_pdf();
        let (bookmarks, total_pages) =
            collect_pdf_bookmarks(&pdf_bytes).expect("action-backed bookmark scan should work");

        assert_eq!(total_pages, 2);
        assert_eq!(bookmarks.len(), 2);
        assert_eq!(bookmarks[0].title, "Pin Diagrams");
        assert_eq!(bookmarks[0].page, 1);
        assert_eq!(bookmarks[0].depth, 0);
        assert_eq!(bookmarks[1].title, "Pinout I/O Descriptions");
        assert_eq!(bookmarks[1].page, 2);
        assert_eq!(bookmarks[1].depth, 0);
    }

    #[test]
    fn rename_overlay_package_in_json_rekeys_the_package_entry() {
        let mut overlay = json!({
            "packages": {
                "48-PIN TQFP": {
                    "pin_count": 48,
                    "pins": { "1": "RA0" },
                    "source": "overlay"
                }
            }
        });

        rename_overlay_package_in_json(&mut overlay, "48-PIN TQFP", "48-PIN TQFP (7x7)")
            .expect("overlay rename should succeed");

        assert!(overlay["packages"].get("48-PIN TQFP").is_none());
        assert_eq!(overlay["packages"]["48-PIN TQFP (7x7)"]["pins"]["1"], "RA0");
    }

    #[test]
    fn rename_overlay_package_in_json_moves_display_name_override() {
        let mut overlay = json!({
            "packages": {
                "48-PIN TQFP": {
                    "pin_count": 48,
                    "pins": { "1": "RA0" },
                    "source": "overlay"
                }
            },
            "display_names": {
                "48-PIN TQFP": "Main TQFP"
            }
        });

        rename_overlay_package_in_json(&mut overlay, "48-PIN TQFP", "48-PIN TQFP (7x7)")
            .expect("overlay rename should move any display-name override");

        assert_eq!(
            overlay["display_names"]["48-PIN TQFP (7x7)"],
            Value::String("Main TQFP".to_string())
        );
        assert!(overlay["display_names"].get("48-PIN TQFP").is_none());
    }

    #[test]
    fn delete_overlay_package_from_json_reports_when_the_file_becomes_empty() {
        let mut overlay = json!({
            "packages": {
                "48-PIN TQFP": {
                    "pin_count": 48,
                    "pins": { "1": "RA0" },
                    "source": "overlay"
                }
            }
        });

        let should_delete_file = delete_overlay_package_from_json(&mut overlay, "48-PIN TQFP")
            .expect("overlay delete should succeed");

        assert!(should_delete_file);
        assert_eq!(overlay["packages"], json!({}));
    }

    #[test]
    fn delete_overlay_package_from_json_keeps_display_name_only_files() {
        let mut overlay = json!({
            "packages": {
                "48-PIN TQFP": {
                    "pin_count": 48,
                    "pins": { "1": "RA0" },
                    "source": "overlay"
                }
            },
            "display_names": {
                "STX04 (48-pin uQFN)": "48-PIN VQFN"
            }
        });

        let should_delete_file = delete_overlay_package_from_json(&mut overlay, "48-PIN TQFP")
            .expect("overlay delete should preserve unrelated display-name overrides");

        assert!(!should_delete_file);
        assert_eq!(
            overlay["display_names"]["STX04 (48-pin uQFN)"],
            "48-PIN VQFN"
        );
    }

    #[test]
    fn set_package_display_name_in_json_adds_and_clears_overrides() {
        let mut overlay = json!({});

        let should_delete_file = set_package_display_name_in_json(
            &mut overlay,
            "STX04 (48-pin uQFN)",
            Some("48-PIN VQFN"),
        )
        .expect("setting a display-name override should succeed");

        assert!(!should_delete_file);
        assert_eq!(
            overlay["display_names"]["STX04 (48-pin uQFN)"],
            "48-PIN VQFN"
        );

        let should_delete_file =
            set_package_display_name_in_json(&mut overlay, "STX04 (48-pin uQFN)", None)
                .expect("clearing a display-name override should succeed");

        assert!(should_delete_file);
    }

    #[test]
    fn openai_array_output_normalizes_to_internal_object_shape() {
        let raw = json!({
            "packages": [
                {
                    "package_name": "64-PIN VQFN-TQFP",
                    "pin_count": 64,
                    "pins": [
                        { "pin_number": 1, "pad_name": "RA0" },
                        { "pin_number": 2, "pad_name": "RA1" }
                    ],
                    "pin_functions": [
                        { "pad_name": "RA0", "functions": ["RA0", "AN0"] },
                        { "pad_name": "RA1", "functions": ["RA1", "AN1"] }
                    ]
                }
            ],
            "corrections": [],
            "notes": ["ok"]
        })
        .to_string();

        let normalized = normalize_openai_output_text(&raw).unwrap();
        let value: Value = serde_json::from_str(&normalized).unwrap();

        assert_eq!(value["packages"]["64-PIN VQFN-TQFP"]["pin_count"], 64);
        assert_eq!(value["packages"]["64-PIN VQFN-TQFP"]["pins"]["1"], "RA0");
        assert_eq!(
            value["packages"]["64-PIN VQFN-TQFP"]["pin_functions"]["RA1"][1],
            "AN1"
        );
    }

    #[test]
    fn openai_incomplete_response_is_reported_explicitly() {
        let result = json!({
            "id": "resp_test",
            "status": "incomplete",
            "incomplete_details": {
                "reason": "max_output_tokens"
            },
            "output": []
        });

        let error = extract_openai_text(&result).unwrap_err();
        assert!(error.contains("OpenAI response incomplete"));
        assert!(error.contains("max_output_tokens"));
    }

    #[test]
    fn openai_streaming_capture_assembles_and_normalizes_long_json() {
        let raw_json = json!({
            "packages": [
                {
                    "package_name": "28-PIN SPDIP",
                    "pin_count": 28,
                    "pins": [
                        { "pin_number": 1, "pad_name": "MCLR" },
                        { "pin_number": 2, "pad_name": "RA0" }
                    ],
                    "pin_functions": [
                        { "pad_name": "MCLR", "functions": ["MCLR"] },
                        { "pad_name": "RA0", "functions": ["RA0", "AN0"] }
                    ]
                }
            ],
            "corrections": [],
            "notes": ["stream ok"]
        })
        .to_string();
        let split = raw_json.len() / 2;
        let first = &raw_json[..split];
        let second = &raw_json[split..];
        let sse = format!(
            "event: response.output_text.delta\n\
data: {{\"type\":\"response.output_text.delta\",\"delta\":{first:?}}}\n\n\
event: response.output_text.delta\n\
data: {{\"type\":\"response.output_text.delta\",\"delta\":{second:?}}}\n\n\
event: response.completed\n\
data: {{\"type\":\"response.completed\",\"response\":{{\"id\":\"resp_test\",\"status\":\"completed\",\"usage\":{{}}}}}}\n\n"
        );

        let normalized =
            parse_openai_stream_reader(std::io::Cursor::new(sse.into_bytes())).unwrap();
        let value: Value = serde_json::from_str(&normalized).unwrap();

        assert_eq!(value["packages"]["28-PIN SPDIP"]["pins"]["2"], "RA0");
        assert_eq!(value["notes"][0], "stream ok");
    }

    #[test]
    fn package_branch_matching_rejects_explicit_wrong_family_branch() {
        assert!(package_matches_selected_device_branch(
            "48-PIN VQFN/TQFP (dsPIC33AKXXXMPS505/dsPIC33AKXXXMPS205)",
            "DSPIC33AK256MPS205"
        ));
        assert!(!package_matches_selected_device_branch(
            "48-PIN VQFN/TQFP (dsPIC33AKXXXMC505/dsPIC33AKXXXMC205)",
            "DSPIC33AK256MPS205"
        ));
        assert!(package_matches_selected_device_branch(
            "48-PIN VQFN",
            "DSPIC33AK256MPS205"
        ));
    }

    #[test]
    fn parse_verifier_response_filters_out_wrong_branch_packages() {
        let raw = json!({
            "packages": {
                "48-PIN VQFN/TQFP (dsPIC33AKXXXMC505/dsPIC33AKXXXMC205)": {
                    "pin_count": 48,
                    "pins": { "1": "RA0" },
                    "pin_functions": { "RA0": ["RA0"] }
                },
                "48-PIN VQFN/TQFP (dsPIC33AKXXXMPS505/dsPIC33AKXXXMPS205)": {
                    "pin_count": 48,
                    "pins": { "1": "RA0" },
                    "pin_functions": { "RA0": ["RA0"] }
                }
            },
            "corrections": [],
            "notes": ["family datasheet"]
        })
        .to_string();
        let device = json!({
            "part_number": "DSPIC33AK256MPS205",
            "selected_package": "STX32 (48-pin VQFN)",
            "pin_count": 48,
            "pins": [
                { "position": 1, "pad_name": "RA0" }
            ]
        });

        let parsed = parse_verifier_response(&raw, &device);

        assert_eq!(parsed.packages.len(), 1);
        assert!(parsed
            .packages
            .contains_key("48-PIN VQFN/TQFP (dsPIC33AKXXXMPS505/dsPIC33AKXXXMPS205)"));
        assert!(!parsed
            .packages
            .contains_key("48-PIN VQFN/TQFP (dsPIC33AKXXXMC505/dsPIC33AKXXXMC205)"));
        assert!(parsed
            .notes
            .iter()
            .any(|note| note.contains("Ignored 1 extracted package table")));
    }

    #[test]
    fn parse_verifier_response_builds_local_function_corrections() {
        let raw = json!({
            "packages": {
                "48-PIN VQFN": {
                    "pin_count": 48,
                    "pins": { "1": "RA0" },
                    "pin_functions": { "RA0": ["RA0", "AN0"] }
                }
            },
            "corrections": [
                {
                    "package": "48-PIN VQFN",
                    "pin_position": 1,
                    "type": "wrong_pad",
                    "note": "provider-generated note that should be ignored"
                }
            ],
            "notes": ["family datasheet"]
        })
        .to_string();
        let device = json!({
            "part_number": "DSPIC33AK256MC205",
            "selected_package": "STX32 (48-pin VQFN)",
            "pin_count": 48,
            "pins": [
                {
                    "position": 1,
                    "pad_name": "RA0",
                    "functions": ["RA0"]
                }
            ]
        });

        let parsed = parse_verifier_response(&raw, &device);
        let corrections = &parsed.packages["48-PIN VQFN"].corrections;

        assert_eq!(corrections.len(), 1);
        assert_eq!(corrections[0].correction_type, "missing_functions");
        assert_eq!(corrections[0].pin_position, 1);
        assert_eq!(corrections[0].datasheet_functions, vec!["RA0", "AN0"]);
        assert!(corrections[0].note.contains("missing fixed functions"));
        assert!(!corrections[0].note.contains("provider-generated"));
    }

    #[test]
    fn verifier_extraction_round_trips_through_cached_json_shape() {
        let raw = "```json\n{\"packages\":{\"48-PIN VQFN\":{\"pin_count\":48,\"pins\":{\"1\":\"RA0\"},\"pin_functions\":{\"RA0\":[\"RA0\",\"AN0\"]}}},\"corrections\":[],\"notes\":[\"cached ok\"],\"clc_input_sources\":[]}\n```";

        let extraction = parse_verifier_extraction(raw).unwrap();
        let cached_json = serde_json::to_value(&extraction).unwrap();
        let restored = verifier_extraction_from_value(&cached_json);

        assert_eq!(restored.packages.len(), 1);
        assert_eq!(restored.packages["48-PIN VQFN"].pins[&1], "RA0");
        assert_eq!(
            restored.packages["48-PIN VQFN"].pin_functions["RA0"],
            vec!["RA0", "AN0"]
        );
        assert_eq!(restored.notes, vec!["cached ok"]);
    }

    #[test]
    fn parse_verifier_response_filters_out_wrong_pin_count_packages() {
        let raw = json!({
            "packages": {
                "48-PIN VQFN": {
                    "pin_count": 48,
                    "pins": { "1": "RA0" },
                    "pin_functions": { "RA0": ["RA0"] }
                },
                "64-PIN TQFP": {
                    "pin_count": 64,
                    "pins": { "1": "RA0" },
                    "pin_functions": { "RA0": ["RA0"] }
                }
            },
            "corrections": [],
            "notes": []
        })
        .to_string();
        let device = json!({
            "part_number": "DSPIC33AK256MC205",
            "selected_package": "STX32 (48-pin VQFN)",
            "pin_count": 48,
            "pins": [
                { "position": 1, "pad_name": "RA0" }
            ]
        });

        let parsed = parse_verifier_response(&raw, &device);

        assert!(parsed.packages.contains_key("48-PIN VQFN"));
        assert!(!parsed.packages.contains_key("64-PIN TQFP"));
        assert!(parsed
            .notes
            .iter()
            .any(|note| note.contains("pin count does not match the selected 48-pin device")));
    }

    #[test]
    fn pinout_cache_scope_is_shared_for_sibling_parts_with_same_pin_count() {
        let mc_device = json!({
            "part_number": "DSPIC33AK256MC205",
            "pin_count": 48
        });
        let mps_device = json!({
            "part_number": "DSPIC33AK256MPS205",
            "pin_count": 48
        });

        assert_eq!(
            verification_cache_scope(Provider::OpenAI, VerifyTask::Pinout, &mc_device),
            verification_cache_scope(Provider::OpenAI, VerifyTask::Pinout, &mps_device)
        );
    }
}
