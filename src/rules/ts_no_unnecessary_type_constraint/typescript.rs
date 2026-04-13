//! ts-no-unnecessary-type-constraint backend — flag `constraint` nodes
//! inside type parameters that constrain to `any` or `unknown`.
//!
//! Detection: walk `type_parameter` nodes and check if their `constraint`
//! child is `any` or `unknown`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "type_parameter" {
        return;
    }
    // Find the `constraint` child node (not a field — tree-sitter uses it as a node kind).
    let constraint = {
        let mut cursor = node.walk();
        let mut found = None;
        for child in node.children(&mut cursor) {
            if child.kind() == "constraint" {
                found = Some(child);
                break;
            }
        }
        found
    };
    let Some(constraint) = constraint else {
        return;
    };
    // The constraint has children: `extends` keyword + type node.
    // Extract the type node (the named child after `extends`).
    let type_text = {
        let mut cursor = constraint.walk();
        let mut text = None;
        for child in constraint.children(&mut cursor) {
            if child.kind() != "extends" {
                text = child.utf8_text(source).ok();
            }
        }
        text
    };
    let Some(text) = type_text else { return };
    let text = text.trim();
    if text != "any" && text != "unknown" {
        return;
    }
    let pos = constraint.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-unnecessary-type-constraint".into(),
        message: format!(
            "Unnecessary `extends {text}` constraint — \
             all types already extend `{text}`."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_extends_any() {
        let diags = run_on("function f<T extends any>(x: T): T { return x; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`any`"));
    }

    #[test]
    fn flags_extends_unknown() {
        let diags = run_on("function f<T extends unknown>(x: T): T { return x; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`unknown`"));
    }

    #[test]
    fn allows_extends_string() {
        assert!(run_on("function f<T extends string>(x: T): T { return x; }").is_empty());
    }
}
