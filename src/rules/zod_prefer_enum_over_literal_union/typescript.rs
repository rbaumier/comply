//! zod-prefer-enum-over-literal-union backend.
//!
//! Walks every `call_expression`. If the callee is `z.union` (or
//! `zod.union`) and the sole argument is an array literal whose every
//! element is a `z.literal('...')` call with a string literal argument,
//! flag the call — `z.enum([...])` is the clearer, more idiomatic form.
//!
//! The check is deliberately strict: any non-literal element, any
//! non-string literal (e.g. `z.literal(1)`, `z.literal(true)`), or a
//! non-array argument disqualifies the match. `z.enum` only accepts a
//! readonly string array, so we refuse to suggest the rewrite unless
//! every branch is a plain string literal.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["z.union", "zod.union"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if name != "z.union" && name != "zod.union" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    if args.named_child_count() != 1 { return; }
    let arr = args.named_child(0).unwrap();
    if arr.kind() != "array" { return; }

    let count = arr.named_child_count();
    if count == 0 { return; }

    for i in 0..count {
        let Some(elem) = arr.named_child(i) else { return; };
        if !is_z_literal_string(elem, source) { return; }
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`z.union([z.literal('...'), ...])` with only string literals — use `z.enum([...])` instead.".into(),
        Severity::Warning,
    ));
}

/// Returns true iff `node` is a `call_expression` of the form
/// `z.literal("...")` or `zod.literal("...")` with a single string
/// literal argument.
fn is_z_literal_string(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" { return false; }
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return false;
    };
    if name != "z.literal" && name != "zod.literal" { return false; }
    let Some(args) = node.child_by_field_name("arguments") else { return false; };
    if args.named_child_count() != 1 { return false; }
    let arg = args.named_child(0).unwrap();
    arg.kind() == "string"
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_union_of_string_literals() {
        assert_eq!(
            run("const s = z.union([z.literal('a'), z.literal('b')]);").len(),
            1
        );
    }

    #[test]
    fn flags_union_of_many_string_literals() {
        assert_eq!(
            run("const s = z.union([z.literal('a'), z.literal('b'), z.literal('c')]);").len(),
            1
        );
    }

    #[test]
    fn flags_zod_alias() {
        assert_eq!(
            run("const s = zod.union([zod.literal('a'), zod.literal('b')]);").len(),
            1
        );
    }

    #[test]
    fn allows_z_enum() {
        assert!(run("const s = z.enum(['a', 'b']);").is_empty());
    }

    #[test]
    fn allows_mixed_literal_types() {
        assert!(
            run("const s = z.union([z.literal('a'), z.literal(1)]);").is_empty()
        );
    }

    #[test]
    fn allows_union_with_non_literal_branch() {
        assert!(
            run("const s = z.union([z.literal('a'), z.string()]);").is_empty()
        );
    }

    #[test]
    fn allows_union_of_number_literals() {
        assert!(
            run("const s = z.union([z.literal(1), z.literal(2)]);").is_empty()
        );
    }

    #[test]
    fn allows_empty_union() {
        // Empty array is degenerate; don't suggest a rewrite.
        assert!(run("const s = z.union([]);").is_empty());
    }
}
