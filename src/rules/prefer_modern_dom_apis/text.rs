use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// True if the byte could be the tail of a callee expression.
fn is_callee_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b')' || b == b']'
}

const PATTERNS: &[(&str, &str, &str)] = &[
    (
        ".insertBefore(",
        "insertBefore",
        "Prefer `ref.before(newNode)` over `parent.insertBefore(newNode, ref)`.",
    ),
    (
        ".replaceChild(",
        "replaceChild",
        "Prefer `old.replaceWith(newNode)` over `parent.replaceChild(newNode, old)`.",
    ),
];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            let bytes = line.as_bytes();
            for &(pattern, _method, message) in PATTERNS {
                let mut start = 0;
                while start + pattern.len() <= bytes.len() {
                    if let Some(rel) = line[start..].find(pattern) {
                        let abs = start + rel;
                        if abs > 0 && is_callee_char(bytes[abs - 1]) {
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: idx + 1,
                                column: abs + 2,
                                rule_id: "prefer-modern-dom-apis".into(),
                                message: message.into(),
                                severity: Severity::Warning,
                            });
                            break; // one per line per pattern
                        }
                        start = abs + pattern.len();
                    } else {
                        break;
                    }
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
    fn flags_insert_before() {
        let d = run("parent.insertBefore(newNode, refNode);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("before"));
    }

    #[test]
    fn flags_replace_child() {
        let d = run("parent.replaceChild(newEl, oldEl);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("replaceWith"));
    }

    #[test]
    fn allows_modern_before() {
        assert!(run("refNode.before(newNode);").is_empty());
    }

    #[test]
    fn allows_modern_replace_with() {
        assert!(run("oldEl.replaceWith(newEl);").is_empty());
    }

    #[test]
    fn ignores_comment() {
        assert!(run("// parent.insertBefore(a, b)").is_empty());
    }
}
