use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ObjectPropertyKind, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True if any descendant of the source range calls `fetch(...)`.
fn subtree_calls_fetch(source: &str, span: oxc_span::Span) -> bool {
    let text = &source[span.start as usize..span.end as usize];
    text.contains("fetch(")
}

/// True if any descendant of the source range accesses `.ok`.
fn subtree_has_ok(source: &str, span: oxc_span::Span) -> bool {
    let text = &source[span.start as usize..span.end as usize];
    text.contains(".ok")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["queryFn"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectExpression(obj) = node.kind() else { return };

        for prop_kind in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(pair) = prop_kind else { continue };
            let key_name = match &pair.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };
            if key_name != "queryFn" {
                continue;
            }

            let value_span = pair.value.span();

            if !subtree_calls_fetch(ctx.source, value_span) {
                continue;
            }
            if subtree_has_ok(ctx.source, value_span) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, pair.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`queryFn` with `fetch()` must check `res.ok` and throw on error so TanStack Query can retry.".into(),
                severity: Severity::Warning,
                span: None,
            });
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
    fn flags_fetch_no_ok_check() {
        assert_eq!(
            run("useQuery({ queryKey: ['x'], queryFn: async () => { const res = await fetch('/api'); return res.json() } })")
                .len(),
            1
        );
    }

    #[test]
    fn allows_with_ok_check() {
        assert!(run(
            "useQuery({ queryKey: ['x'], queryFn: async () => { const res = await fetch('/api'); if (!res.ok) throw new Error('err'); return res.json() } })"
        )
        .is_empty());
    }
}
