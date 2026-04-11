use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Known encoding identifiers and their canonical lowercase form.
const ENCODINGS: &[(&str, &str)] = &[
    ("UTF-8", "utf-8"),
    ("Utf-8", "utf-8"),
    ("UTF8", "utf8"),
    ("Utf8", "utf8"),
    ("ASCII", "ascii"),
    ("Ascii", "ascii"),
];

/// Scan `line` for quoted encoding identifiers with wrong casing.
/// Returns (column_0based, bad_value, replacement) for each match.
fn find_bad_encodings(line: &str) -> Vec<(usize, &'static str, &'static str)> {
    let mut results = Vec::new();
    let bytes = line.as_bytes();

    for &(bad, good) in ENCODINGS {
        let mut start = 0;
        while start + bad.len() + 2 <= bytes.len() {
            if let Some(pos) = line[start..].find(bad) {
                let abs = start + pos;
                // Must be inside quotes: check char before and after
                if abs > 0 && abs + bad.len() < bytes.len() {
                    let before = bytes[abs - 1];
                    let after = bytes[abs + bad.len()];
                    if (before == b'\'' || before == b'"' || before == b'`') && before == after {
                        results.push((abs, bad, good));
                    }
                }
                start = abs + bad.len();
            } else {
                break;
            }
        }
    }
    results
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }

            for (col, bad, good) in find_bad_encodings(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "text-encoding-identifier-case".into(),
                    message: format!("Prefer `'{good}'` over `'{bad}'`."),
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
    fn flags_uppercase_utf8_dash() {
        let d = run(r#"const enc = "UTF-8";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("utf-8"));
    }

    #[test]
    fn flags_mixed_case_utf8() {
        let d = run(r#"const enc = 'Utf-8';"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("utf-8"));
    }

    #[test]
    fn flags_uppercase_ascii() {
        let d = run(r#"const enc = "ASCII";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("ascii"));
    }

    #[test]
    fn flags_uppercase_utf8_no_dash() {
        let d = run(r#"new TextDecoder("UTF8");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("utf8"));
    }

    #[test]
    fn allows_lowercase_utf8() {
        assert!(run(r#"const enc = "utf-8";"#).is_empty());
    }

    #[test]
    fn allows_lowercase_ascii() {
        assert!(run(r#"const enc = 'ascii';"#).is_empty());
    }

    #[test]
    fn allows_lowercase_utf8_no_dash() {
        assert!(run(r#"const enc = 'utf8';"#).is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run(r#"// "UTF-8" is wrong"#).is_empty());
    }

    #[test]
    fn flags_in_backticks() {
        let d = run(r#"const enc = `UTF-8`;"#);
        assert_eq!(d.len(), 1);
    }
}
