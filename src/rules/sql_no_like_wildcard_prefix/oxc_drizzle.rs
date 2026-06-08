use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        let name = id.name.as_str();
        if name != "like" && name != "ilike" {
            return;
        }
        let Some(second) = call.arguments.get(1) else {
            return;
        };
        let starts_with_percent = match second {
            Argument::StringLiteral(lit) => lit.value.as_str().starts_with('%'),
            Argument::TemplateLiteral(tpl) => tpl
                .quasis
                .first()
                .is_some_and(|q| q.value.raw.as_str().starts_with('%')),
            _ => false,
        };
        if !starts_with_percent {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`like(col, '%...')` prevents index usage — use \
                      full-text search instead."
                .into(),
            severity: Severity::Warning,
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

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_like_with_leading_wildcard() {
        assert_eq!(run_on("db.select().from(users).where(like(users.name, '%john%'));").len(), 1);
    }

    #[test]
    fn flags_ilike_with_leading_wildcard() {
        assert_eq!(run_on("db.select().from(users).where(ilike(users.email, '%@gmail.com'));").len(), 1);
    }

    #[test]
    fn allows_suffix_wildcard() {
        assert!(run_on("db.select().from(users).where(like(users.name, 'john%'));").is_empty());
    }

    #[test]
    fn allows_eq_call() {
        assert!(run_on("db.select().from(users).where(eq(users.name, 'john'));").is_empty());
    }
}
