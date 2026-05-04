use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSAsExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSAsExpression(as_expr) = node.kind() else {
            return;
        };
        // The inner expression must be a call to JSON.parse
        let Expression::CallExpression(call) = &as_expr.expression else {
            return;
        };
        let is_json_parse = match &call.callee {
            Expression::StaticMemberExpression(mem) => {
                if let Expression::Identifier(obj) = &mem.object {
                    obj.name.as_str() == "JSON" && mem.property.name.as_str() == "parse"
                } else {
                    false
                }
            }
            _ => false,
        };
        if !is_json_parse {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, as_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Casting `JSON.parse(...) as T` is a lie — the \
                      runtime shape may not match T. Validate with a \
                      Zod schema (`Schema.safeParse(JSON.parse(raw))`) \
                      or a type guard function that inspects the value."
                .into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_json_parse_as_type() {
        assert_eq!(run_on("const u = JSON.parse(raw) as User;").len(), 1);
    }

    #[test]
    fn allows_json_parse_with_schema() {
        assert!(run_on("const u = UserSchema.parse(JSON.parse(raw));").is_empty());
    }

    #[test]
    fn allows_other_cast() {
        assert!(run_on("const u = value as User;").is_empty());
    }

    #[test]
    fn does_not_flag_other_function_call_cast() {
        assert!(run_on("const u = getRaw() as User;").is_empty());
    }
}
