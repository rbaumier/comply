use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// True if the byte could be the tail of a callee expression.
fn is_callee_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b')' || b == b']'
}

/// Methods that, when called with a `data-*` first argument, should use `.dataset` instead.
const PATTERNS: &[(&str, &str)] = &[
    (".setAttribute(", "setAttribute"),
    (".getAttribute(", "getAttribute"),
    (".removeAttribute(", "removeAttribute"),
    (".hasAttribute(", "hasAttribute"),
];

/// Check if the string argument following the opening paren starts with a
/// `data-` prefix. Looks for patterns like `('data-` or `("data-`.
fn has_data_attr_arg(line: &str, paren_pos: usize) -> bool {
    let rest = &line[paren_pos..];
    rest.starts_with("('data-")
        || rest.starts_with("(\"data-")
        || rest.starts_with("(`data-")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            let bytes = line.as_bytes();
            for &(pattern, method) in PATTERNS {
                let mut start = 0;
                while start + pattern.len() <= bytes.len() {
                    if let Some(rel) = line[start..].find(pattern) {
                        let abs = start + rel;
                        let paren_pos = abs + pattern.len() - 1; // position of '('
                        if abs > 0
                            && is_callee_char(bytes[abs - 1])
                            && paren_pos + 7 < bytes.len()
                            && has_data_attr_arg(line, paren_pos)
                        {
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: idx + 1,
                                column: abs + 2,
                                rule_id: "prefer-dom-node-dataset".into(),
                                message: format!(
                                    "Prefer `.dataset` over `.{}(…)` for `data-*` attributes.",
                                    method
                                ),
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
    fn flags_set_attribute_data() {
        let d = run(r#"el.setAttribute('data-foo', 'bar');"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("setAttribute"));
    }

    #[test]
    fn flags_get_attribute_data() {
        let d = run(r#"const v = el.getAttribute("data-id");"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("getAttribute"));
    }

    #[test]
    fn flags_remove_attribute_data() {
        let d = run(r#"el.removeAttribute('data-temp');"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("removeAttribute"));
    }

    #[test]
    fn allows_non_data_attribute() {
        assert!(run(r#"el.setAttribute('class', 'active');"#).is_empty());
    }

    #[test]
    fn allows_dataset() {
        assert!(run(r#"el.dataset.foo = 'bar';"#).is_empty());
    }

    #[test]
    fn ignores_comment() {
        assert!(run(r#"// el.setAttribute('data-x', v)"#).is_empty());
    }
}
