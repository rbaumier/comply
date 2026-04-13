use crate::diagnostic::{Diagnostic, Severity};

/// Walk past a balanced parenthesised group starting at `bytes[start]` == `(`.
fn skip_parens(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 1u32;
    let mut i = start + 1;
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        i += 1;
    }
    if depth == 0 { Some(i) } else { None }
}

/// Detect `.indexOf(…) !== -1`, `.indexOf(…) != -1`, `.indexOf(…) > -1`,
/// `.indexOf(…) >= 0`, `.indexOf(…) === -1`, `.indexOf(…) == -1`,
/// `.indexOf(…) < 0`.
fn has_indexof_existence_check(line: &str) -> bool {
    for method in &[".indexOf(", ".lastIndexOf("] {
        let mut start = 0;
        while let Some(pos) = line[start..].find(method) {
            let open_paren = start + pos + method.len() - 1;
            let bytes = line.as_bytes();
            if let Some(after_paren) = skip_parens(bytes, open_paren) {
                let rest = line[after_paren..].trim_start();
                if rest.starts_with("!== -1")
                    || rest.starts_with("!= -1")
                    || rest.starts_with("> -1")
                    || rest.starts_with(">= 0")
                    || rest.starts_with("=== -1")
                    || rest.starts_with("== -1")
                    || rest.starts_with("< 0")
                {
                    return true;
                }
            }
            start = open_paren + 1;
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
        if has_indexof_existence_check(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "prefer-includes".into(),
                message: "Prefer `.includes(x)` over `.indexOf(x) !== -1` — more readable.".into(),
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
    fn flags_indexof_not_equal_minus_one() {
        assert_eq!(run_ts("if (arr.indexOf(x) !== -1) {}").len(), 1);
    }

    #[test]
    fn flags_indexof_loose_not_equal() {
        assert_eq!(run_ts("if (arr.indexOf(x) != -1) {}").len(), 1);
    }

    #[test]
    fn flags_indexof_gte_zero() {
        assert_eq!(run_ts("if (arr.indexOf(x) >= 0) {}").len(), 1);
    }

    #[test]
    fn flags_lastindexof() {
        assert_eq!(run_ts("if (str.lastIndexOf(c) !== -1) {}").len(), 1);
    }

    #[test]
    fn allows_includes() {
        assert!(run_ts("if (arr.includes(x)) {}").is_empty());
    }

    #[test]
    fn allows_indexof_other_comparison() {
        assert!(run_ts("if (arr.indexOf(x) === 2) {}").is_empty());
    }
}
