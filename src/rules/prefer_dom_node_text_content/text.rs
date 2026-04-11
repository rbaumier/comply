use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            let bytes = line.as_bytes();
            let pattern = ".innerText";
            let pat_len = pattern.len();
            let mut start = 0;
            while start + pat_len <= bytes.len() {
                if let Some(rel) = line[start..].find(pattern) {
                    let abs = start + rel;
                    let after = abs + pat_len;
                    // Verify it's a property access, not part of a longer identifier.
                    // The char after `.innerText` must NOT be alphanumeric/underscore.
                    let after_ok = after >= bytes.len()
                        || (!bytes[after].is_ascii_alphanumeric() && bytes[after] != b'_');
                    if after_ok {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: abs + 2,
                            rule_id: "prefer-dom-node-text-content".into(),
                            message: "Prefer `.textContent` over `.innerText`.".into(),
                            severity: Severity::Warning,
                        });
                        break; // one per line
                    }
                    start = abs + pat_len;
                } else {
                    break;
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(s: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), s))
    }

    #[test]
    fn flags_inner_text_read() {
        let d = run("const t = el.innerText;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("textContent"));
    }

    #[test]
    fn flags_inner_text_assign() {
        let d = run(r#"el.innerText = "hello";"#);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_text_content() {
        assert!(run("const t = el.textContent;").is_empty());
    }

    #[test]
    fn ignores_inner_text_html() {
        // `.innerTextHTML` or similar longer identifier should not match
        assert!(run("el.innerTextHTML = 'x';").is_empty());
    }

    #[test]
    fn ignores_comment() {
        assert!(run("// el.innerText is deprecated").is_empty());
    }
}
