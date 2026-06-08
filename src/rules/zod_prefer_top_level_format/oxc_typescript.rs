use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const STRING_CHAIN_METHODS: &[(&str, &str)] = &[
    ("email", "z.email()"),
    ("url", "z.url()"),
    ("uuid", "z.uuid()"),
    ("cuid", "z.cuid()"),
    ("ulid", "z.ulid()"),
    ("datetime", "z.iso.datetime()"),
    ("date", "z.iso.date()"),
    ("time", "z.iso.time()"),
    ("ip", "z.ipv4() or z.ipv6()"),
];

fn is_z_method_call<'a>(expr: &'a oxc_ast::ast::Expression<'a>, method: &str) -> bool {
    let oxc_ast::ast::Expression::CallExpression(call) = expr else {
        return false;
    };
    let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
        return false;
    };
    obj.name.as_str() == "z" && member.property.name.as_str() == method
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.string", "z.number"])
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
        let oxc_ast::ast::Expression::StaticMemberExpression(outer_member) = &call.callee else {
            return;
        };
        let method_name = outer_member.property.name.as_str();

        if let Some((_, replacement)) = STRING_CHAIN_METHODS.iter().find(|(m, _)| *m == method_name) {
            if is_z_method_call(&outer_member.object, "string") {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`z.string().{method_name}()` — use `{replacement}` \
                         directly. Top-level format helpers are shorter, \
                         faster, and tree-shakeable."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        } else if method_name == "int" && is_z_method_call(&outer_member.object, "number") {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`z.number().int()` — use `z.int()` directly.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_string_email() {
        assert_eq!(run("const s = z.string().email();").len(), 1);
    }

    #[test]
    fn flags_string_url() {
        assert_eq!(run("const s = z.string().url();").len(), 1);
    }

    #[test]
    fn flags_number_int() {
        assert_eq!(run("const s = z.number().int();").len(), 1);
    }

    #[test]
    fn allows_top_level_format() {
        assert!(run("const s = z.email();").is_empty());
        assert!(run("const s = z.int();").is_empty());
    }

    #[test]
    fn allows_plain_string_schema() {
        assert!(run("const s = z.string();").is_empty());
    }
}
