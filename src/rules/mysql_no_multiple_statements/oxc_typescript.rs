use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["mysql"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "mysql" {
            return;
        }
        let prop = member.property.name.as_str();
        if prop != "createConnection" && prop != "createPool" {
            return;
        }

        for arg in &call.arguments {
            let Argument::ObjectExpression(obj_expr) = arg else { continue };
            for prop_kind in &obj_expr.properties {
                let ObjectPropertyKind::ObjectProperty(pair) = prop_kind else { continue };
                let key_name = match &pair.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    PropertyKey::StringLiteral(s) => s.value.as_str(),
                    _ => continue,
                };
                if key_name != "multipleStatements" {
                    continue;
                }
                let Expression::BooleanLiteral(val) = &pair.value else { continue };
                if !val.value {
                    continue;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, pair.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message:
                        "`multipleStatements: true` amplifies SQL injection risk \u{2014} remove this option."
                            .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_create_connection_multiple_statements_true() {
        assert_eq!(
            run("mysql.createConnection({ host: 'localhost', multipleStatements: true })").len(),
            1
        );
    }

    #[test]
    fn flags_create_pool_multiple_statements_true() {
        assert_eq!(
            run("mysql.createPool({ multipleStatements: true, host: 'localhost' })").len(),
            1
        );
    }

    #[test]
    fn allows_multiple_statements_false() {
        assert!(run("mysql.createConnection({ multipleStatements: false })").is_empty());
    }

    #[test]
    fn allows_missing_option() {
        assert!(run("mysql.createConnection({ host: 'localhost' })").is_empty());
    }

    #[test]
    fn ignores_other_callers() {
        assert!(run("db.createConnection({ multipleStatements: true })").is_empty());
    }
}
