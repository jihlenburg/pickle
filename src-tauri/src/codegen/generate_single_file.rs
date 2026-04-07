//! Single-translation-unit helpers for compile-check and export workflows.
//!
//! The main generator emits paired `.h` / `.c` outputs. Some workflows still
//! need a merged C unit, so this module owns the header-inline merge logic and
//! the extraction of signal alias macros from the generated header.

pub(crate) fn merge_generated_files(
    header_name: &str,
    header_content: &str,
    source_content: &str,
) -> String {
    let defines = extract_signal_alias_defines(header_content);
    let mut merged =
        source_content.replace(&format!("#include \"{}\"", header_name), "#include <xc.h>");

    if !defines.is_empty() {
        // Family-specific compile-checks use a single translation unit, so inline the signal-name
        // macros that would normally come from the generated header.
        let define_block = defines.join("\n");
        merged = merged.replace(
            "#include <xc.h>\n",
            &format!("#include <xc.h>\n\n{}\n", define_block),
        );
    }

    merged
}

fn extract_signal_alias_defines(header_content: &str) -> Vec<String> {
    let mut defines = Vec::new();
    let mut in_defines = false;
    for line in header_content.lines() {
        if line.starts_with("#define ")
            && (line.contains("_PORT") || line.contains("_LAT") || line.contains("_TRIS"))
        {
            defines.push(line.to_string());
        }
        if line.starts_with("/* ---") && line.contains("Signal name") {
            in_defines = true;
        }
        if in_defines {
            defines.push(line.to_string());
            if line.is_empty() {
                in_defines = false;
            }
        }
    }
    defines
}
