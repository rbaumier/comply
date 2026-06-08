//! ts-no-duplicate-type-constituents oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSUnionType, AstType::TSIntersectionType]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (members_text, kind_label, span_start) = match node.kind() {
            AstKind::TSUnionType(u) => {
                let texts: Vec<&str> = u
                    .types
                    .iter()
                    .map(|t| {
                        ctx.source[t.span().start as usize..t.span().end as usize].trim()
                    })
                    .collect();
                (texts, "union", u.span.start)
            }
            AstKind::TSIntersectionType(i) => {
                let texts: Vec<&str> = i
                    .types
                    .iter()
                    .map(|t| {
                        ctx.source[t.span().start as usize..t.span().end as usize].trim()
                    })
                    .collect();
                (texts, "intersection", i.span.start)
            }
            _ => return,
        };

        let mut seen: HashSet<&str> = HashSet::new();
        let mut duplicates: Vec<&str> = Vec::new();
        for member in &members_text {
            if !seen.insert(*member) {
                duplicates.push(*member);
            }
        }
        if duplicates.is_empty() {
            return;
        }

        let dup = duplicates[0];
        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Duplicate member `{dup}` in {kind_label} type — remove the repeat."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

use oxc_span::GetSpan;

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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_duplicate_in_union() {
        let src = r#"type A = string | number | string;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_duplicate_in_intersection() {
        let src = r#"type A = Foo & Bar & Foo;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_distinct_members() {
        let src = r#"type A = string | number | boolean;"#;
        assert!(run(src).is_empty());
    }
}
