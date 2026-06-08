use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.custom", "zod.custom"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
            return;
        };
        let obj_name = obj.name.as_str();
        if (obj_name != "z" && obj_name != "zod") || member.property.name.as_str() != "custom" {
            return;
        }
        if !call.arguments.is_empty() {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`z.custom()` without a validator function performs no runtime check — \
                      provide a validator function to z.custom()."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_empty_z_custom() {
        assert_eq!(run("const s = z.custom();").len(), 1);
    }

    #[test]
    fn flags_empty_z_custom_with_type_arg() {
        assert_eq!(run("const s = z.custom<string>();").len(), 1);
    }

    #[test]
    fn flags_empty_zod_custom() {
        assert_eq!(run("const s = zod.custom();").len(), 1);
    }

    #[test]
    fn allows_z_custom_with_validator() {
        assert!(run("const s = z.custom<string>((v) => typeof v === 'string');").is_empty());
    }

    #[test]
    fn allows_unrelated_calls() {
        assert!(run("const s = z.string();").is_empty());
        assert!(run("const s = custom();").is_empty());
    }
}
