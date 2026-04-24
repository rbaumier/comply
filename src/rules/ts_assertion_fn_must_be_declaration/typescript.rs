//! Flags arrow functions whose declared return type is an
//! `asserts` type predicate.

use crate::diagnostic::{Diagnostic, Severity};

fn return_type_has_asserts(arrow: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(rt) = arrow.child_by_field_name("return_type") else { return false };
    let text = std::str::from_utf8(&source[rt.byte_range()]).unwrap_or("");
    // `: asserts x is T` or `: asserts x`
    text.contains("asserts ")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "arrow_function" {
        return;
    }
    if !return_type_has_asserts(node, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Assertion functions (`asserts`) must be declared with `function`, not as an arrow.".into(),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_arrow_with_asserts_predicate() {
        let src = "const assertIsString = (x: unknown): asserts x is string => { if (typeof x !== 'string') throw 0; };";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_arrow_with_bare_asserts() {
        let src = "const check = (x: unknown): asserts x => { if (!x) throw 0; };";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_function_declaration_with_asserts() {
        let src = "function assertIsString(x: unknown): asserts x is string { if (typeof x !== 'string') throw 0; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_regular_arrow() {
        let src = "const f = (x: number): string => String(x);";
        assert!(run(src).is_empty());
    }
}
