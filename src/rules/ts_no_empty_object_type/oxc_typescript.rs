//! OxcCheck backend for ts-no-empty-object-type — flag `{}` used as a type.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn source_line_at(source: &str, byte_offset: usize) -> &str {
    let start = source[..byte_offset].rfind('\n').map_or(0, |i| i + 1);
    let end = source[byte_offset..]
        .find('\n')
        .map_or(source.len(), |i| byte_offset + i);
    &source[start..end]
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSTypeLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSTypeLiteral(lit) = node.kind() else { return };
        if !lit.members.is_empty() {
            return;
        }

        // Skip `{}` when used as a type argument to TaggedError (better-result convention).
        let parent = semantic.nodes().parent_node(node.id());
        if matches!(parent.kind(), AstKind::TSTypeParameterInstantiation(_)) {
            let line_src = source_line_at(ctx.source, lit.span.start as usize);
            if line_src.contains("TaggedError") {
                return;
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, lit.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`{}` as a type matches any non-nullish value. \
                      Use `Record<string, never>` for an empty object, \
                      or `object` / `unknown`."
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_empty_object_type() {
        assert_eq!(run_on("const x: {} = {};").len(), 1);
    }

    #[test]
    fn allows_tagged_error_empty_type_param() {
        assert!(run_on(r#"export class FooError extends TaggedError("foo")<{}>() {}"#).is_empty());
    }

    #[test]
    fn still_flags_empty_object_in_other_generics() {
        assert_eq!(run_on("type X = Map<string, {}>;").len(), 1);
    }
}
