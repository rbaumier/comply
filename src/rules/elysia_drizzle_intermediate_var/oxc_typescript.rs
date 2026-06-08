//! elysia-drizzle-intermediate-var OXC backend — flag inline
//! `t.Omit/Pick(createInsertSchema(...))`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // callee must be t.Omit or t.Pick
        let callee_text = match &call.callee {
            Expression::StaticMemberExpression(m) => {
                if let Expression::Identifier(obj) = &m.object {
                    if obj.name != "t" {
                        return;
                    }
                    let prop = m.property.name.as_str();
                    if prop != "Omit" && prop != "Pick" {
                        return;
                    }
                    format!("t.{prop}")
                } else {
                    return;
                }
            }
            _ => return,
        };

        // first argument must be a call to createInsertSchema / createSelectSchema
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(arg_expr) = first_arg.as_expression() else {
            return;
        };
        let Expression::CallExpression(inner_call) = arg_expr else {
            return;
        };
        let Expression::Identifier(inner_ident) = &inner_call.callee else {
            return;
        };
        let inner_name = inner_ident.name.as_str();
        if inner_name != "createInsertSchema" && inner_name != "createSelectSchema" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Inline `{callee_text}({inner_name}(...))` causes infinite type instantiation — bind to a variable first."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_inline_omit() {
        let src = "import { createInsertSchema } from 'drizzle-typebox';\nconst body = t.Omit(createInsertSchema(users), ['id']);";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_inline_pick() {
        let src = "import { createInsertSchema } from 'drizzle-typebox';\nconst body = t.Pick(createInsertSchema(users), ['name']);";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_intermediate_variable() {
        let src = "import { createInsertSchema } from 'drizzle-typebox';\nconst schema = createInsertSchema(users);\nconst body = t.Omit(schema, ['id']);";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_drizzle_files() {
        let src = "const body = t.Omit(createInsertSchema(users), ['id']);";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
