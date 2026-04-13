use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects useless quantifiers:
/// - `{1}` — matches exactly once anyway
/// - `{1,1}` — same
/// - Quantifier on an empty group `()+`
fn find_useless_quantifiers(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Detect `{1}` or `{1,1}`
        if bytes[i] == b'{' {
            let start = i;
            let mut j = i + 1;
            let mut num_buf = String::new();
            while j < len && bytes[j].is_ascii_digit() {
                num_buf.push(bytes[j] as char);
                j += 1;
            }
            if j < len && bytes[j] == b'}' && num_buf == "1" {
                hits.push(start);
            } else if j < len && bytes[j] == b',' {
                j += 1;
                let mut num_buf2 = String::new();
                while j < len && bytes[j].is_ascii_digit() {
                    num_buf2.push(bytes[j] as char);
                    j += 1;
                }
                if j < len && bytes[j] == b'}' && num_buf == "1" && num_buf2 == "1" {
                    hits.push(start);
                }
            }
        }

        // Detect quantifier on empty group: ()+, ()*, ()?
        if bytes[i] == b'(' && i + 2 < len && bytes[i + 1] == b')' {
            let after = bytes[i + 2];
            if after == b'+' || after == b'*' || after == b'?' || after == b'{' {
                hits.push(i);
            }
        }

        i += 1;
    }
    hits
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_useless_quantifiers(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-useless-quantifier".into(),
                    message: "Useless quantifier \u{2014} it can only match once or matches an empty element.".into(),
                    severity: Severity::Warning,
                    span: None,
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
    fn flags_quantifier_one() {
        assert_eq!(run(r#"const re = /a{1}/;"#).len(), 1);
    }

    #[test]
    fn flags_quantifier_one_one() {
        assert_eq!(run(r#"const re = /a{1,1}/;"#).len(), 1);
    }

    #[test]
    fn allows_meaningful_quantifier() {
        assert!(run(r#"const re = /a{2}/;"#).is_empty());
    }

    #[test]
    fn flags_empty_group_quantified() {
        assert_eq!(run(r#"const re = /()+/;"#).len(), 1);
    }
}
