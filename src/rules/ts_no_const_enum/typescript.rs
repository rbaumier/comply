//! ts-no-const-enum backend — walk `enum_declaration` nodes and flag those
//! whose children contain a `const` keyword token.
//!
//! Tree-sitter-typescript represents `const enum Foo {}` as an
//! `enum_declaration` with an anonymous `const` child token preceding the
//! `enum` keyword. We iterate every child (named + anonymous) to find it.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = source;
    if node.kind() != "enum_declaration" {
        return;
    }
    let mut cursor = node.walk();
    let has_const = node
        .children(&mut cursor)
        .any(|child| child.kind() == "const");
    if !has_const {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-const-enum".into(),
        message: "`const enum` is inlined at compile time and breaks with \
                  isolatedModules; use a regular enum or a union type instead."
            .into(),
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
    fn flags_const_enum() {
        let diags = run_on("const enum E { A, B }");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "ts-no-const-enum");
    }

    #[test]
    fn allows_regular_enum() {
        assert!(run_on("enum E { A, B }").is_empty());
    }

    #[test]
    fn flags_declare_const_enum() {
        let diags = run_on("declare const enum E { A, B }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_exported_const_enum() {
        let diags = run_on("export const enum E { A, B }");
        assert_eq!(diags.len(), 1);
    }
}
