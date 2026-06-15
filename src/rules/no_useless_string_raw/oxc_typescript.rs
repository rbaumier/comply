//! no-useless-string-raw oxc backend for TypeScript / JavaScript / TSX.
//!
//! Flags `` String.raw`...` `` tagged templates whose template contains no
//! backslash. Without a backslash escape, the `String.raw` tag changes nothing
//! and the plain template literal should be used. Interpolations (`${expr}`)
//! are irrelevant — only the raw chunk text decides. Only the static-member
//! tag form `String.raw` is matched; a `String.raw(...)` call is left alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// True when `expr` is the static member access `String.raw`.
fn is_string_raw_tag(expr: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    let Expression::Identifier(object) = &member.object else {
        return false;
    };
    object.name.as_str() == "String" && member.property.name.as_str() == "raw"
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TaggedTemplateExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["String.raw"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TaggedTemplateExpression(tagged) = node.kind() else {
            return;
        };
        if !is_string_raw_tag(&tagged.tag) {
            return;
        }
        // `.value.raw` keeps backslashes verbatim (`\n` is two chars). Any
        // backslash means an escape the plain template would alter, so the tag
        // is doing real work and the template must stay tagged.
        let has_backslash = tagged
            .quasi
            .quasis
            .iter()
            .any(|q| q.value.raw.as_str().contains('\\'));
        if has_backslash {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, tagged.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`String.raw` is useless when the template has no backslash escape.".into(),
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

    // Biome invalid fixtures: a String.raw tagged template with no backslash.
    #[test]
    fn flags_string_raw_without_backslash() {
        assert_eq!(run_on("String.raw`a`;").len(), 1);
    }

    #[test]
    fn flags_string_raw_with_interpolation_no_backslash() {
        assert_eq!(run_on("String.raw`a ${v}`;").len(), 1);
    }

    // Biome valid fixtures: any backslash in the template keeps it tagged.
    #[test]
    fn ignores_template_with_newline_escape() {
        assert!(run_on("String.raw`\\n`;").is_empty());
    }

    #[test]
    fn ignores_template_with_backslash_and_interpolation() {
        assert!(run_on("String.raw`\\n ${a}`;").is_empty());
    }

    #[test]
    fn ignores_template_with_tab_escape() {
        assert!(run_on("String.raw`a\\tb`;").is_empty());
    }

    #[test]
    fn ignores_template_with_double_backslash() {
        assert!(run_on("String.raw`a\\\\b`;").is_empty());
    }

    // A `String.raw(...)` call is not a tagged template — out of scope.
    #[test]
    fn ignores_string_raw_call_form() {
        assert!(run_on("String.raw({ raw: ['a'] });").is_empty());
    }

    // A different tag is unrelated.
    #[test]
    fn ignores_other_tag() {
        assert!(run_on("css`a`;").is_empty());
    }

    // A plain untagged template is unrelated.
    #[test]
    fn ignores_plain_template() {
        assert!(run_on("const s = `a ${v}`;").is_empty());
    }

    // `raw` on something other than `String` must not fire.
    #[test]
    fn ignores_non_string_object_raw() {
        assert!(run_on("other.raw`a`;").is_empty());
    }
}
