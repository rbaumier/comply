//! zod-no-throw-in-refine backend — flag `throw` inside `.refine()` /
//! `.superRefine()` callbacks.
//!
//! Why: throwing inside a Zod refinement bypasses Zod's issue
//! aggregation. The exception escapes `parse()` / `safeParse()` instead
//! of being reported as a validation issue on the expected field.
//!
//! Detection: walk from a `throw_statement` up the tree. Find the
//! nearest enclosing function (arrow_function / function_expression).
//! That function must be an argument to a `call_expression` whose
//! callee is a `member_expression` with property `refine` or
//! `superRefine`.

use crate::diagnostic::{Diagnostic, Severity};

fn callback_is_refine_arg(func_node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    // Expected parent chain: function -> arguments -> call_expression.
    let Some(arguments) = func_node.parent() else {
        return false;
    };
    if arguments.kind() != "arguments" {
        return false;
    }
    let Some(call) = arguments.parent() else {
        return false;
    };
    if call.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    let name = prop.utf8_text(source).unwrap_or("");
    name == "refine" || name == "superRefine"
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "throw_statement" {
        return;
    }

    // Walk up to the nearest enclosing function. If we leave the
    // function (e.g. into a class body or module) without finding a
    // refine callback, bail.
    let mut cursor = node.parent();
    while let Some(parent) = cursor {
        match parent.kind() {
            "arrow_function" | "function_expression" | "function_declaration" | "method_definition" => {
                if callback_is_refine_arg(parent, source) {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "zod-no-throw-in-refine".into(),
                        message: "`throw` inside a Zod `.refine()` / `.superRefine()` bypasses \
                                  issue aggregation — use `ctx.addIssue()` in superRefine, or \
                                  return `false` in refine."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                return;
            }
            _ => {
                cursor = parent.parent();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_throw_in_refine() {
        let src = r#"
            const schema = z.string().refine((val) => {
                if (val.length < 3) throw new Error("too short");
                return true;
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_throw_in_super_refine() {
        let src = r#"
            const schema = z.object({ a: z.string() }).superRefine((val, ctx) => {
                throw new Error("nope");
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_ctx_add_issue_in_super_refine() {
        let src = r#"
            const schema = z.object({ a: z.string() }).superRefine((val, ctx) => {
                ctx.addIssue({ code: "custom", message: "bad" });
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_return_false_in_refine() {
        let src = r#"
            const schema = z.string().refine((val) => {
                if (val.length < 3) return false;
                return true;
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_throw_outside_refine() {
        let src = r#"
            function validate(val) {
                if (!val) throw new Error("missing");
                return val;
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_throw_literal_in_refine() {
        let src = r#"
            z.string().refine((val) => {
                throw "bad";
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }
}
