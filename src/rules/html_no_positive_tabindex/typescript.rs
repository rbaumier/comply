//! html-no-positive-tabindex AST backend.
//!
//! Flags the lowercase HTML `tabindex` attribute when its numeric value
//! is greater than zero. JSX canonically uses camelCase `tabIndex`, but
//! developers sometimes write HTML-style `tabindex` in JSX; either way
//! a positive value disrupts the natural focus order.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true if the attribute value is a numeric literal > 0.
fn is_positive(attr: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(val) = crate::rules::jsx::jsx_attribute_value(attr) else {
        return false;
    };
    let Ok(text) = val.utf8_text(source) else {
        return false;
    };
    // JSX expression: {N}
    if let Some(inner) = text.strip_prefix('{').and_then(|s| s.strip_suffix('}'))
        && let Ok(n) = inner.trim().parse::<i32>()
    {
        return n > 0;
    }
    // String literal: "N"
    let unquoted = text.trim_matches(|c| c == '"' || c == '\'');
    if let Ok(n) = unquoted.trim().parse::<i32>() {
        return n > 0;
    }
    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_attribute" {
        return;
    }
    if crate::rules::jsx::jsx_attribute_name(node, source) != Some("tabindex") {
        return;
    }
    if !is_positive(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "html-no-positive-tabindex".into(),
        message: "`tabindex` must not be positive — use `0` or `-1`.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_positive_tabindex_string() {
        assert_eq!(run(r#"const x = <div tabindex="5" />;"#).len(), 1);
    }

    #[test]
    fn flags_positive_tabindex_expr() {
        assert_eq!(run(r#"const x = <div tabindex={3} />;"#).len(), 1);
    }

    #[test]
    fn allows_zero() {
        assert!(run(r#"const x = <div tabindex="0" />;"#).is_empty());
    }

    #[test]
    fn allows_negative() {
        assert!(run(r#"const x = <div tabindex={-1} />;"#).is_empty());
    }
}
