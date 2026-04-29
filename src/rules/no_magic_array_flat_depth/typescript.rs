//! no-magic-array-flat-depth AST backend — flag `arr.flat(N)` where `N`
//! is a numeric literal other than `1`. `Infinity` is allowed (it has
//! semantic meaning) and so are non-literal arguments (a named constant
//! or expression).
//!
//! Walks `call_expression` nodes whose callee is `<x>.flat` and whose
//! single argument is a `number` literal.

use crate::diagnostic::{Diagnostic, Severity};

/// True if `callee` is a member expression `<x>.flat`.
fn is_flat_call(callee: tree_sitter::Node, source: &[u8]) -> bool {
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    prop.utf8_text(source).unwrap_or("") == "flat"
}

/// True if the argument node is a numeric literal that is NOT `1`.
fn is_magic_depth(arg: tree_sitter::Node, source: &[u8]) -> bool {
    if arg.kind() != "number" {
        return false;
    }
    let text = arg.utf8_text(source).unwrap_or("").trim();
    if let Ok(val) = text.parse::<f64>() {
        (val - 1.0).abs() >= f64::EPSILON
    } else {
        false
    }
}

crate::ast_check! { on ["call_expression"] prefilter = ["flat"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if !is_flat_call(callee, source) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let Some(first) = args.named_children(&mut cursor).next() else { return };
    if !is_magic_depth(first, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-magic-array-flat-depth",
        "Magic number as `.flat()` depth is not allowed. Use a named constant or `Infinity`.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_magic_number_depth() {
        assert_eq!(run_on("arr.flat(3);").len(), 1);
    }

    #[test]
    fn flags_magic_number_depth_two() {
        assert_eq!(run_on("arr.flat(2);").len(), 1);
    }

    #[test]
    fn flags_magic_number_depth_large() {
        assert_eq!(run_on("const result = items.flat(10);").len(), 1);
    }

    #[test]
    fn allows_flat_without_args() {
        assert!(run_on("arr.flat();").is_empty());
    }

    #[test]
    fn allows_flat_depth_one() {
        assert!(run_on("arr.flat(1);").is_empty());
    }

    #[test]
    fn allows_flat_infinity() {
        assert!(run_on("arr.flat(Infinity);").is_empty());
    }

    #[test]
    fn allows_flat_variable() {
        assert!(run_on("arr.flat(depth);").is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run_on("// arr.flat(3);").is_empty());
    }
}
