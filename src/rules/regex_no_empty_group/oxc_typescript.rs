//! regex-no-empty-group OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn has_empty_group(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'(' && bytes[i + 1] == b')' {
            return true;
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
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_empty_group_in_literal() {
        assert_eq!(run_on("const re = /foo()/;").len(), 1);
    }


    #[test]
    fn allows_non_empty_group() {
        assert!(run_on("const re = /foo(bar)/;").is_empty());
    }


    #[test]
    fn ignores_tailwind_class_string() {
        assert!(run_on(r#"const x = "has-[>svg]:()";"#).is_empty());
    }


    #[test]
    fn ignores_url_string() {
        assert!(run_on(r#"const u = "http://a/b()";"#).is_empty());
    }


    #[test]
    fn ignores_import_path() {
        assert!(run_on(r#"import X from "@scope/pkg/sub";"#).is_empty());
    }
}
