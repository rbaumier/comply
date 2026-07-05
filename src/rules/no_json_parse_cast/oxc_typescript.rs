use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

/// True when `callee` is the static member `JSON.<method>` (object identifier
/// `JSON`, property `method`).
fn is_json_static_call(callee: &Expression, method: &str) -> bool {
    let Expression::StaticMemberExpression(mem) = callee else {
        return false;
    };
    matches!(&mem.object, Expression::Identifier(obj) if obj.name.as_str() == "JSON")
        && mem.property.name.as_str() == method
}

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
        if !is_json_static_call(&call.callee, "parse") {
            return;
        }
        // Skip the deep-clone idiom `JSON.parse(JSON.stringify(x))`: a
        // serialize/parse round-trip of an already-typed value, not a parse of
        // external data, so `as T` is a truthful assertion.
        if let Some(Argument::CallExpression(inner)) = call.arguments.first() {
            if is_json_static_call(&inner.callee, "stringify") {
                return;
            }
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    #[test]
    fn allows_json_parse_stringify_deep_clone() {
        assert!(
            run_on("const steps = JSON.parse(JSON.stringify(commandSteps)) as NotificationStep[];")
                .is_empty()
        );
    }

    #[test]
    fn flags_json_parse_of_external_string() {
        assert_eq!(run_on("const data = JSON.parse(raw) as Foo;").len(), 1);
    }

    #[test]
    fn flags_json_parse_of_local_storage() {
        assert_eq!(
            run_on("const cfg = JSON.parse(localStorage.getItem('k')!) as Config;").len(),
            1
        );
    }
}
