//! regex-no-empty-group OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// True when `pattern` contains an empty capturing group `()` outside a
/// character class. Inside `[...]`, `(` and `)` are literal members, not a
/// group, so the scanner tracks an `in_class` flag (set on an unescaped `[`,
/// cleared on an unescaped `]`) and only flags `()` while outside a class.
fn has_empty_group(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    let mut in_class = false;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'[' if !in_class => in_class = true,
            b']' if in_class => in_class = false,
            b'(' if !in_class && bytes.get(i + 1) == Some(&b')') => return true,
            _ => {}
        }
        i += 1;
    }
    false
}

pub struct Check;

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
        let AstKind::RegExpLiteral(regexp) = node.kind() else {
            return;
        };
        let pattern = regexp.regex.pattern.text.as_str();
        if !has_empty_group(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, regexp.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Empty capturing group `()` in regex \u{2014} add a pattern or remove it."
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
    fn flags_empty_group_in_literal() {
        assert_eq!(run_on("const re = /foo()/;").len(), 1);
    }

    #[test]
    fn allows_non_empty_group() {
        assert!(run_on("const re = /foo(bar)/;").is_empty());
    }

    // --- Character-class context: `()` inside `[...]` are literal chars,
    // not a capturing group, so they must not be flagged (issue #3773). ---

    #[test]
    fn ignores_parens_in_character_class() {
        assert!(run_on(r#"const re = /[\s"'():;\\/\[\]{}]/;"#).is_empty());
    }

    #[test]
    fn ignores_parens_in_character_class_variant() {
        assert!(run_on(r#"const re = /[;"'\\/\[\](){}]/;"#).is_empty());
    }

    #[test]
    fn ignores_parens_in_character_class_router_shape() {
        assert!(run_on(r#"const re = /[.\\+*[^\]$()]/g;"#).is_empty());
    }

    #[test]
    fn ignores_bare_class_with_parens() {
        assert!(run_on("const re = /[()]/;").is_empty());
    }

    #[test]
    fn flags_empty_group_after_character_class() {
        assert_eq!(run_on("const re = /[abc]()/;").len(), 1);
    }
}
