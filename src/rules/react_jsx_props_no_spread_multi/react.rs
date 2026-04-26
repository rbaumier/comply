//! react-jsx-props-no-spread-multi AST backend.
//!
//! Flags the same identifier being spread (`{...obj}`) more than once
//! on a single JSX element.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

crate::ast_check! { on ["jsx_self_closing_element", "jsx_opening_element"] => |node, source, ctx, diagnostics|
    let mut seen_spreads = HashSet::new();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_expression" {
            continue;
        }
        // Look for spread: { ...expr }
        let mut inner_cursor = child.walk();
        for inner in child.children(&mut inner_cursor) {
            if inner.kind() == "spread_element" {
                let Some(arg) = inner.child(1) else { continue };
                let Ok(arg_text) = arg.utf8_text(source) else { continue };
                if !seen_spreads.insert(arg_text.to_string()) {
                    let pos = child.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "react-jsx-props-no-spread-multi".into(),
                        message: format!(
                            "`{arg_text}` is spread multiple times on this element."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_duplicate_spread() {
        let src = "const x = <div {...props} {...props} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_different_spreads() {
        let src = "const x = <div {...a} {...b} />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_spread() {
        let src = "const x = <div {...props} />;";
        assert!(run(src).is_empty());
    }
}
