//! OxcCheck backend for zod-no-throw-in-refine — flag `throw` inside
//! `.refine()` / `.superRefine()` callbacks.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ThrowStatement]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["refine", "superRefine"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ThrowStatement(throw) = node.kind() else {
            return;
        };

        // Walk ancestors to find the nearest enclosing function.
        for ancestor in semantic.nodes().ancestors(node.id()) {
            match ancestor.kind() {
                AstKind::ArrowFunctionExpression(_) | AstKind::Function(_) => {
                    if callback_is_refine_arg(ancestor, semantic) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, throw.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
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
                _ => {}
            }
        }
    }
}

fn callback_is_refine_arg<'a>(
    func_node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    // Parent chain: function -> Argument position in CallExpression
    let parent = semantic.nodes().parent_node(func_node.id());
    let AstKind::CallExpression(call) = parent.kind() else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let name = member.property.name.as_str();
    name == "refine" || name == "superRefine"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
