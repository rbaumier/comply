//! OxcCheck backend for regex-prefer-quantifier.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Tokenize a regex pattern into elements (single chars or escape
/// sequences like `\d`). Character classes `[...]`, groups `(` `)`,
/// alternation `|`, and `{m,n}` quantifiers are emitted as opaque
/// tokens so they never participate in repetition runs.
fn tokenize(pattern: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            let next_len = pattern[i + 1..].chars().next().map_or(1, |c| c.len_utf8());
            tokens.push(&pattern[i..i + 1 + next_len]);
            i += 1 + next_len;
        } else if bytes[i] == b'[' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b']' {
                if bytes[i] == b'\\' {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < bytes.len() {
                i += 1;
            }
            tokens.push(&pattern[start..i]);
        } else if bytes[i] == b'(' || bytes[i] == b')' || bytes[i] == b'|' {
            tokens.push(&pattern[i..i + 1]);
            i += 1;
        } else if bytes[i] == b'{' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'}' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            tokens.push(&pattern[start..i]);
        } else {
            let ch_len = pattern[i..].chars().next().map_or(1, |c| c.len_utf8());
            tokens.push(&pattern[i..i + ch_len]);
            i += ch_len;
        }
    }
    tokens
}

fn has_repeated_tokens(pattern: &str) -> bool {
    let tokens = tokenize(pattern);
    let mut run = 1;
    for i in 1..tokens.len() {
        let prev = tokens[i - 1];
        let cur = tokens[i];
        if cur == prev
            && !matches!(cur, "(" | ")" | "|" | "?" | "+" | "*" | "^" | "$" | ".")
            && !cur.starts_with('{')
            && !cur.starts_with('[')
        {
            run += 1;
            if run >= 3 {
                return true;
            }
        } else {
            run = 1;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(regex) = node.kind() else {
            return;
        };

        let pattern = regex.regex.pattern.text.as_str();
        if !has_repeated_tokens(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, regex.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Repeated identical pattern in regex \u{2014} use a quantifier like `a{3}` or `\\d{4}`."
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
    fn flags_repeated_chars() {
        assert_eq!(run_on("const re = /aaa/;").len(), 1);
    }

    #[test]
    fn flags_repeated_escape() {
        assert_eq!(run_on(r#"const re = /\d\d\d\d/;"#).len(), 1);
    }

    #[test]
    fn allows_two_chars() {
        assert!(run_on("const re = /aa/;").is_empty());
    }

    #[test]
    fn allows_quantifier_already() {
        assert!(run_on("const re = /a{3}/;").is_empty());
    }

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        assert!(run_on(r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        assert!(run_on(r#"const u = "http://a/aaa/b";"#).is_empty());
    }

    #[test]
    fn no_panic_on_multibyte_chars() {
        assert!(run_on(r#"const re = /cabinets vétérinaires/;"#).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        assert!(run_on(r#"import X from "@tanstack/react-query";"#).is_empty());
    }
}
