//! OXC backend for escape-case — flag lowercase hex digits in escape sequences.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use regex::Regex;
use std::sync::{Arc, LazyLock};

static RE_ESCAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\\(x[0-9A-Fa-f]{2}|u[0-9A-Fa-f]{4}|u\{[0-9A-Fa-f]+\})").unwrap());

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[
            oxc_ast::AstType::StringLiteral,
            oxc_ast::AstType::TemplateLiteral,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::StringLiteral(lit) => {
                let text = &ctx.source[lit.span.start as usize..lit.span.end as usize];
                check_escapes(text, lit.span.start as usize, ctx, diagnostics);
            }
            AstKind::TemplateLiteral(tpl) => {
                let text = &ctx.source[tpl.span.start as usize..tpl.span.end as usize];
                check_escapes(text, tpl.span.start as usize, ctx, diagnostics);
            }
            _ => {}
        }
    }
}

fn check_escapes(text: &str, byte_start: usize, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    for mat in RE_ESCAPE.find_iter(text) {
        let matched = mat.as_str();
        let body = &matched[1..];

        if !has_lowercase_hex(body) {
            continue;
        }

        let prefix = &text[..mat.start()];
        let trailing_bs = prefix.len() - prefix.trim_end_matches('\\').len();
        if trailing_bs % 2 == 1 {
            continue;
        }

        let uppercased = format!("\\{}", uppercase_hex(body));
        let abs_offset = byte_start + mat.start();
        let (line, column) = byte_offset_to_line_col(ctx.source, abs_offset);

        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "escape-case".into(),
            message: format!(
                "Use uppercase characters for the value of the escape \
                 sequence: `{matched}` -> `{uppercased}`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn has_lowercase_hex(s: &str) -> bool {
    s.chars()
        .any(|c| c.is_ascii_lowercase() && c.is_ascii_hexdigit())
}

fn uppercase_hex(body: &str) -> String {
    body.chars()
        .map(|c| {
            if c.is_ascii_hexdigit() && c.is_ascii_lowercase() {
                c.to_ascii_uppercase()
            } else {
                c
            }
        })
        .collect()
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
    fn flags_lowercase_hex_escape() {
        let d = run_on(r#"const a = "\xff";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\xFF"));
    }

    #[test]
    fn flags_lowercase_unicode_escape() {
        let d = run_on(r#"const a = "\u00ff";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\u00FF"));
    }

    #[test]
    fn flags_lowercase_unicode_brace_escape() {
        let d = run_on(r#"const a = "\u{1a2b}";"#);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(r"\u{1A2B}"));
    }

    #[test]
    fn allows_uppercase_escape() {
        assert!(run_on(r#"const a = "\xFF";"#).is_empty());
    }

    #[test]
    fn allows_uppercase_unicode() {
        assert!(run_on(r#"const a = "\u00FF";"#).is_empty());
    }

    #[test]
    fn flags_multiple_on_one_line() {
        let d = run_on(r#"const a = "\xff\u00ab";"#);
        assert_eq!(d.len(), 2);
    }
}
