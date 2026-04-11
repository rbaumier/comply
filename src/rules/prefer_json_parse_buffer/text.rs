//! prefer-json-parse-buffer — flag `JSON.parse(fs.readFileSync(path, 'utf-8'))`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `JSON.parse(` followed by `readFileSync(` with a utf-8 encoding argument on the same line.
fn find_json_parse_readfilesync(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let needle = "JSON.parse(";
    let mut start = 0;

    while let Some(parse_pos) = line[start..].find(needle) {
        let abs = start + parse_pos;
        let after_parse = &line[abs + needle.len()..];

        // Look for `readFileSync(` or `fs.readFileSync(` (any prefix.readFileSync)
        if let Some(rfs_pos) = after_parse.find("readFileSync(") {
            let after_rfs = &after_parse[rfs_pos + "readFileSync(".len()..];
            // Look for a comma followed by utf-8 encoding
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
    // Find a comma (after the first arg)
    let Some(comma_pos) = s.find(',') else {
        return false;
    };
    let after_comma = s[comma_pos + 1..].trim();

    // Check for string 'utf-8', "utf-8", 'utf8', "utf8"
    for enc in ["'utf-8'", "\"utf-8\"", "'utf8'", "\"utf8\""] {
        if after_comma.starts_with(enc) {
            return true;
        }
    }

    // Check for { encoding: 'utf-8' } pattern
    if after_comma.starts_with('{') {
        let lower = after_comma.to_ascii_lowercase();
        if lower.contains("encoding") && (lower.contains("utf-8") || lower.contains("utf8")) {
            return true;
        }
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
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
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_readfilesync_utf8() {
        let d = run(r#"const data = JSON.parse(fs.readFileSync('config.json', 'utf-8'));"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-json-parse-buffer");
    }

    #[test]
    fn flags_readfilesync_utf8_no_dash() {
        let d = run(r#"const data = JSON.parse(fs.readFileSync('config.json', 'utf8'));"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_bare_readfilesync() {
        let d = run(r#"JSON.parse(readFileSync(path, "utf-8"))"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_readfilesync_without_encoding() {
        assert!(run(r#"JSON.parse(fs.readFileSync('config.json'))"#).is_empty());
    }

    #[test]
    fn allows_non_utf8_encoding() {
        assert!(run(r#"JSON.parse(fs.readFileSync('file', 'ascii'))"#).is_empty());
    }
}
