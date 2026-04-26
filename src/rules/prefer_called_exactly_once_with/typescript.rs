//! prefer-called-exactly-once-with — detect consecutive
//! `expect(x).toHaveBeenCalledTimes(1)` followed by
//! `expect(x).toHaveBeenCalledWith(...)` sharing the same `x`.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// Info extracted from an `expect(x).<matcher>(args)` call expression.
struct ExpectCall<'a> {
    /// Source text of the argument passed to `expect(...)`.
    expect_arg: &'a str,
    /// Matcher name (e.g. `toHaveBeenCalledTimes`).
    matcher: &'a str,
    /// The arguments node of the matcher call (so we can read its named children).
    matcher_args: Node<'a>,
}

/// If `stmt` is an expression statement shaped like `expect(x).MATCHER(args)`,
/// return the decomposed parts. Otherwise return None.
fn parse_expect_call<'a>(stmt: Node<'a>, source: &'a [u8]) -> Option<ExpectCall<'a>> {
    if stmt.kind() != "expression_statement" {
        return None;
    }
    let call = stmt.named_child(0)?;
    if call.kind() != "call_expression" {
        return None;
    }
    let callee = call.child_by_field_name("function")?;
    if callee.kind() != "member_expression" {
        return None;
    }
    let matcher_prop = callee.child_by_field_name("property")?;
    let matcher = matcher_prop.utf8_text(source).ok()?;

    let expect_call = callee.child_by_field_name("object")?;
    if expect_call.kind() != "call_expression" {
        return None;
    }
    let expect_ident = expect_call.child_by_field_name("function")?;
    if expect_ident.utf8_text(source).ok()? != "expect" {
        return None;
    }
    let expect_args = expect_call.child_by_field_name("arguments")?;
    if expect_args.named_child_count() != 1 {
        return None;
    }
    let expect_arg_node = expect_args.named_child(0)?;
    let expect_arg = expect_arg_node.utf8_text(source).ok()?;

    let matcher_args = call.child_by_field_name("arguments")?;

    Some(ExpectCall {
        expect_arg,
        matcher,
        matcher_args,
    })
}

/// True if `args` is a single argument `1` (number literal).
fn is_literal_one(args: Node<'_>, source: &[u8]) -> bool {
    if args.named_child_count() != 1 {
        return false;
    }
    let Some(arg) = args.named_child(0) else {
        return false;
    };
    if arg.kind() != "number" {
        return false;
    }
    arg.utf8_text(source).map(str::trim).unwrap_or("") == "1"
}

crate::ast_check! { on ["statement_block", "program"] => |node, source, ctx, diagnostics|
    // We scan any node that can contain a sequence of sibling statements.
    // `program` is the file root; `statement_block` is `{ ... }` bodies.
    let count = node.named_child_count();
    if count < 2 {
        return;
    }

    for i in 0..count - 1 {
        let Some(first) = node.named_child(i) else { continue };
        let Some(second) = node.named_child(i + 1) else { continue };

        let Some(a) = parse_expect_call(first, source) else { continue };
        if a.matcher != "toHaveBeenCalledTimes" { continue }
        if !is_literal_one(a.matcher_args, source) { continue }

        let Some(b) = parse_expect_call(second, source) else { continue };
        if b.matcher != "toHaveBeenCalledWith" { continue }

        // Require the same `expect(x)` argument in both statements.
        if a.expect_arg != b.expect_arg { continue }

        let pos = first.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "prefer-called-exactly-once-with".into(),
            message: format!(
                "Replace `toHaveBeenCalledTimes(1)` + `toHaveBeenCalledWith(...)` on `{}` with `toHaveBeenCalledExactlyOnceWith(...)`.",
                a.expect_arg
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_ts;

    #[test]
    fn flags_times_one_then_called_with_same_mock() {
        let src = r#"
            test('x', () => {
                expect(fn).toHaveBeenCalledTimes(1);
                expect(fn).toHaveBeenCalledWith(1, 2);
            });
        "#;
        let d = run_ts(src, &Check);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("toHaveBeenCalledExactlyOnceWith"));
    }

    #[test]
    fn ignores_non_consecutive_statements() {
        let src = r#"
            test('x', () => {
                expect(fn).toHaveBeenCalledTimes(1);
                doSomething();
                expect(fn).toHaveBeenCalledWith(1, 2);
            });
        "#;
        let d = run_ts(src, &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_different_mocks() {
        let src = r#"
            test('x', () => {
                expect(a).toHaveBeenCalledTimes(1);
                expect(b).toHaveBeenCalledWith(1);
            });
        "#;
        let d = run_ts(src, &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_times_not_equal_to_one() {
        let src = r#"
            test('x', () => {
                expect(fn).toHaveBeenCalledTimes(2);
                expect(fn).toHaveBeenCalledWith(1);
            });
        "#;
        let d = run_ts(src, &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_reversed_order() {
        let src = r#"
            test('x', () => {
                expect(fn).toHaveBeenCalledWith(1);
                expect(fn).toHaveBeenCalledTimes(1);
            });
        "#;
        let d = run_ts(src, &Check);
        assert!(d.is_empty());
    }

    #[test]
    fn flags_at_program_top_level() {
        let src = "expect(fn).toHaveBeenCalledTimes(1);\nexpect(fn).toHaveBeenCalledWith(42);\n";
        let d = run_ts(src, &Check);
        assert_eq!(d.len(), 1);
    }
}
