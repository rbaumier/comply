use crate::diagnostic::{Diagnostic, Severity};

/// Detect `JSON.parse(` followed by `readFileSync(` with a utf-8 encoding argument.
fn find_json_parse_readfilesync(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let needle = "JSON.parse(";
    let mut start = 0;

    while let Some(parse_pos) = line[start..].find(needle) {
        let abs = start + parse_pos;
        let after_parse = &line[abs + needle.len()..];

        if let Some(rfs_pos) = after_parse.find("readFileSync(") {
            let after_rfs = &after_parse[rfs_pos + "readFileSync(".len()..];
            if has_utf8_encoding_arg(after_rfs) {
                hits.push(abs);
            }
        }

        start = abs + needle.len();
    }
    hits
}

/// Check if the text after `readFileSync(path` contains `, 'utf-8'` or `, "utf8"` etc.
fn has_utf8_encoding_arg(s: &str) -> bool {
    let Some(comma_pos) = s.find(',') else {
        return false;
    };
    let after_comma = s[comma_pos + 1..].trim();

    for enc in ["'utf-8'", "\"utf-8\"", "'utf8'", "\"utf8\""] {
        if after_comma.starts_with(enc) {
            return true;
        }
    }

    if after_comma.starts_with('{') {
        let lower = after_comma.to_ascii_lowercase();
        if lower.contains("encoding") && (lower.contains("utf-8") || lower.contains("utf8")) {
            return true;
        }
    }

    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }
    let src = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in src.lines().enumerate() {
        for col in find_json_parse_readfilesync(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: col + 1,
                rule_id: "prefer-json-parse-buffer".into(),
                message:
                    "Prefer reading the JSON file as a buffer — remove the encoding argument."
                        .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_readfilesync_utf8() {
        let d = run_ts(r#"const data = JSON.parse(fs.readFileSync('config.json', 'utf-8'));"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-json-parse-buffer");
    }

    #[test]
    fn flags_readfilesync_utf8_no_dash() {
        let d = run_ts(r#"const data = JSON.parse(fs.readFileSync('config.json', 'utf8'));"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_readfilesync_without_encoding() {
        assert!(run_ts(r#"JSON.parse(fs.readFileSync('config.json'))"#).is_empty());
    }

    #[test]
    fn allows_non_utf8_encoding() {
        assert!(run_ts(r#"JSON.parse(fs.readFileSync('file', 'ascii'))"#).is_empty());
    }
}
