use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// True if the byte could be the tail of a callee expression.
fn is_callee_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b')' || b == b']'
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
            let pattern = ".removeChild(";
            let mut start = 0;
            while start + pattern.len() <= bytes.len() {
                if let Some(rel) = line[start..].find(pattern) {
                    let abs = start + rel;
                    if abs > 0 && is_callee_char(bytes[abs - 1]) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: abs + 2,
                            rule_id: "prefer-dom-node-remove".into(),
                            message: "Prefer `childNode.remove()` over `parentNode.removeChild(childNode)`.".into(),
                            severity: Severity::Warning,
                        });
                        break;
                    }
                    start = abs + pattern.len();
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
    fn flags_remove_child() {
        let d = run("parent.removeChild(child);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("remove"));
    }

    #[test]
    fn flags_parent_node_remove_child() {
        let d = run("el.parentNode.removeChild(el);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_remove() {
        assert!(run("child.remove();").is_empty());
    }

    #[test]
    fn ignores_comment() {
        assert!(run("// parent.removeChild(child)").is_empty());
    }
}
