use crate::diagnostic::{Diagnostic, Severity};

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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }
    let src = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in src.lines().enumerate() {
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
                        break;
                    }
                    start = abs + pattern.len();
                } else {
                    break;
                }
            }
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
    fn flags_insert_before() {
        let d = run_ts("parent.insertBefore(newNode, refNode);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("before"));
    }

    #[test]
    fn flags_replace_child() {
        let d = run_ts("parent.replaceChild(newEl, oldEl);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("replaceWith"));
    }

    #[test]
    fn allows_modern_before() {
        assert!(run_ts("refNode.before(newNode);").is_empty());
    }

    #[test]
    fn allows_modern_replace_with() {
        assert!(run_ts("oldEl.replaceWith(newEl);").is_empty());
    }

    #[test]
    fn ignores_comment() {
        assert!(run_ts("// parent.insertBefore(a, b)").is_empty());
    }
}
