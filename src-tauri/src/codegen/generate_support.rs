//! Shared formatting and text-emission helpers for the C generator.
//!
//! The main generator phases all need the same comment alignment, section
//! framing, signal-name normalization, and fuse-pragmas filtering behavior.
//! Keeping those mechanics here prevents the phase logic from being buried in
//! formatting details.

use once_cell::sync::Lazy;
use regex::Regex;

use crate::codegen::oscillator::{managed_config_fields_for_device, OscConfig};

const COMMENT_COL: usize = 40;

static SAFE_SIGNAL_NAME_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[^A-Za-z0-9_]").unwrap());

pub(crate) fn push_section_comment(lines: &mut Vec<String>, title: &str, body: &[&str]) {
    lines.push(
        "/* ---------------------------------------------------------------------------".into(),
    );
    lines.push(format!(" * {}", title));
    for line in body {
        lines.push(format!(" * {}", line));
    }
    lines.push(
        " * -------------------------------------------------------------------------*/".into(),
    );
}

pub(crate) fn extend_aligned_sections(lines: &mut Vec<String>, text: &str) {
    let mut sections: Vec<Vec<String>> = vec![vec![]];
    for line in text.lines() {
        if line.is_empty() {
            sections.push(vec![]);
        } else {
            sections
                .last_mut()
                .expect("sections always contains at least one block")
                .push(line.to_string());
        }
    }

    for (index, section) in sections.iter().enumerate() {
        if !section.is_empty() {
            lines.extend(align_comments(section));
        }
        if index + 1 < sections.len() {
            lines.push(String::new());
        }
    }
}

fn pragma_config_field(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("#pragma config ")?;
    let (field, _) = rest.split_once('=')?;
    Some(field.trim())
}

pub(crate) fn filter_fuse_pragmas_for_oscillator(
    fuse_text: &str,
    part_number: &str,
    osc_config: Option<&OscConfig>,
) -> Option<String> {
    let trimmed = fuse_text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let Some(osc) = osc_config else {
        return Some(trimmed.to_string());
    };

    let excluded_fields = managed_config_fields_for_device(part_number, osc);
    if excluded_fields.is_empty() {
        return Some(trimmed.to_string());
    }

    let kept_sections: Vec<String> = trimmed
        .split("\n\n")
        .filter_map(|section| {
            let mut kept_lines = Vec::new();
            let mut kept_pragmas = 0usize;

            for line in section.lines() {
                if let Some(field) = pragma_config_field(line) {
                    if excluded_fields.contains(field) {
                        continue;
                    }
                    kept_pragmas += 1;
                }
                kept_lines.push(line.to_string());
            }

            if kept_pragmas == 0 {
                None
            } else {
                Some(kept_lines.join("\n"))
            }
        })
        .collect();

    if kept_sections.is_empty() {
        None
    } else {
        Some(kept_sections.join("\n\n"))
    }
}

pub(crate) fn sanitize_signal_name(name: &str) -> String {
    SAFE_SIGNAL_NAME_RE.replace_all(name, "_").to_uppercase()
}

/// Align inline comments in a block of C statements to a consistent column.
pub(crate) fn align_comments(lines: &[String]) -> Vec<String> {
    let mut parsed: Vec<(String, Option<String>)> = Vec::new();

    for line in lines {
        let stripped = line.trim_start();
        if stripped.starts_with("/*") || stripped.starts_with('*') || !line.contains("/*") {
            parsed.push((line.clone(), None));
            continue;
        }
        if let Some(idx) = line.find("/*") {
            let code_part = line[..idx].trim_end().to_string();
            let comment_part = line[idx..].to_string();
            parsed.push((code_part, Some(comment_part)));
        } else {
            parsed.push((line.clone(), None));
        }
    }

    let mut max_code_len = COMMENT_COL;
    for (code, comment) in &parsed {
        if comment.is_some() {
            max_code_len = max_code_len.max(code.len() + 2);
        }
    }

    parsed
        .into_iter()
        .map(|(code, comment)| {
            if let Some(c) = comment {
                let padding = max_code_len.saturating_sub(code.len());
                format!("{}{}{}", code, " ".repeat(padding), c)
            } else {
                code
            }
        })
        .collect()
}
