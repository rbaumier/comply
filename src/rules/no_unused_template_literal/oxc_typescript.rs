//! no-unused-template-literal OXC backend.
//!
//! Flags a template literal that has no substitutions and needs no
//! template-specific character handling — i.e. `` `bar` `` should be the
//! plain string literal `"bar"`. Ported from Biome's
//! `noUnusedTemplateLiteral`.
//!
//! A template literal is flagged when ALL of these hold:
//! - it is not the quasi of a tagged template (`` tag`bar` ``),
//! - it has no `${…}` interpolations, and
//! - its raw chunk text contains no real newline byte, single-quote, or
//!   double-quote byte (those would need escaping or a multiline form in a
//!   plain string, so the template form earns its keep).
//!
//! The check inspects the *raw* source text of each quasi, so an escape
//! written as `\n`/`\r`/` ` is just backslash-letters — it carries no
//! literal newline byte and stays flagged, matching Biome.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TemplateLiteral(tpl) = node.kind() else {
            return;
        };

        // Tagged templates (`` tag`bar` ``) carry meaning in the tag and are
        // never reducible to a string literal — skip them.
        let parent = semantic.nodes().parent_node(node.id());
        if matches!(parent.kind(), AstKind::TaggedTemplateExpression(_)) {
            return;
        }

        // Any `${…}` substitution means the template is doing real work.
        if !tpl.expressions.is_empty() {
            return;
        }

        // A real newline, single-quote, or double-quote byte in the raw text
        // would force escaping or a multiline form in a plain string, so the
        // backticks are justified.
        let needs_template = tpl.quasis.iter().any(|quasi| {
            quasi
                .value
                .raw
                .bytes()
                .any(|byte| matches!(byte, b'\n' | b'\'' | b'"'))
        });
        if needs_template {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, tpl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Template literal without interpolation or special characters \u{2014} use a string literal."
                .into(),
            severity: Severity::Error,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // ── Biome invalid.js fixtures — every one must flag ──────────────

    #[test]
    fn flags_plain_backtick_string() {
        assert_eq!(run("var foo = /*1*/`bar`/*2*/;").len(), 1);
    }

    #[test]
    fn flags_trailing_space() {
        assert_eq!(run("var foo1 = `bar `;").len(), 1);
    }

    #[test]
    fn flags_carriage_return_escape() {
        // `\r` written as an escape is backslash-r, no real newline byte.
        assert_eq!(run(r"var foo = `back\rtick`;").len(), 1);
    }

    #[test]
    fn flags_newline_escape() {
        // `\n` as an escape sequence, not a literal newline.
        assert_eq!(run(r"var foo = `back\ntick`;").len(), 1);
    }

    #[test]
    fn flags_unicode_line_separator_escape() {
        assert_eq!(run(r"var foo = `back\u2028tick`").len(), 1);
    }

    #[test]
    fn flags_unicode_paragraph_separator_escape() {
        assert_eq!(run(r"var foo = `back\u2029tick`;").len(), 1);
    }

    #[test]
    fn flags_escaped_backslashes_then_newline_escape() {
        assert_eq!(run(r"var foo = `back\\\\\ntick`;").len(), 1);
    }

    #[test]
    fn flags_lone_newline_escape() {
        assert_eq!(run(r"var foo = `\n`;").len(), 1);
    }

    #[test]
    fn flags_directive_like_template() {
        assert_eq!(run("function foo() { `use strict`; foo(); }").len(), 1);
    }

    #[test]
    fn flags_escaped_backslash_n() {
        assert_eq!(run(r"var foo = `foo\\nbar`;").len(), 1);
    }

    #[test]
    fn flags_escaped_backslash_newline_escape() {
        assert_eq!(run(r"var foo = `foo\\\nbar`;").len(), 1);
    }

    #[test]
    fn flags_many_escaped_backslashes() {
        assert_eq!(run(r"var foo = `foo\\\\\\\nbar`;").len(), 1);
    }

    // ── Biome valid.js fixtures — none may flag ──────────────────────

    #[test]
    fn allows_literal_newline() {
        assert!(run("var foo2 = `bar\nhas newline`;").is_empty());
    }

    #[test]
    fn allows_escaped_double_quotes() {
        // `\"bar\"` — the `"` bytes are present in the raw text.
        assert!(run(r#"var foo3 = `\"bar\"`"#).is_empty());
    }

    #[test]
    fn allows_single_quotes() {
        assert!(run("var foo4 = `'bar'`").is_empty());
    }

    #[test]
    fn allows_inner_single_quotes() {
        assert!(run("var foo = `bar 'baz'`;").is_empty());
    }

    #[test]
    fn allows_interpolation() {
        assert!(run("var foo = `back${x}tick`;").is_empty());
    }

    #[test]
    fn allows_tagged_template() {
        assert!(run("var foo = tag`backtick`;").is_empty());
    }

    #[test]
    fn allows_template_with_trailing_literal_newline() {
        assert!(run("var foo = `something \nelse`;").is_empty());
    }
}
