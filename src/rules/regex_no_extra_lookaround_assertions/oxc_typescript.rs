//! regex-no-extra-lookaround-assertions OXC backend — flag useless nested
//! lookaround assertions that can be inlined (e.g. `(?=(?=a))`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn has_extra_lookaround(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 4 < len {
        if bytes[i] == b'(' && bytes[i + 1] == b'?' {
            let is_lookaround = matches!(bytes[i + 2], b'=' | b'!')
                || (bytes[i + 2] == b'<' && i + 3 < len && matches!(bytes[i + 3], b'=' | b'!'));

            if is_lookaround {
                let content_start = if bytes[i + 2] == b'<' { i + 4 } else { i + 3 };
                if content_start < len {
                    let trimmed = &pattern[content_start..];
                    if (trimmed.starts_with("(?=")
                        || trimmed.starts_with("(?!")
                        || trimmed.starts_with("(?<=")
                        || trimmed.starts_with("(?<!"))
                        && let Some(inner_close) = find_matching_paren(bytes, content_start)
                        && inner_close + 1 < len
                        && bytes[inner_close + 1] == b')'
                    {
                        return true;
                    }
                }
            }
        }
        i += 1;
    }
    false
}

fn find_matching_paren(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 1;
    let mut j = start + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
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
        let AstKind::RegExpLiteral(re) = node.kind() else { return };
        let pattern = re.regex.pattern.text.as_str();
        if !has_extra_lookaround(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Useless nested lookaround assertion \u{2014} it can be inlined.".into(),
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
    fn flags_nested_lookahead() {
        assert_eq!(run_on(r#"const re = /(?=(?=a))/;"#).len(), 1);
    }


    #[test]
    fn allows_single_lookahead() {
        assert!(run_on(r#"const re = /(?=a)/;"#).is_empty());
    }


    #[test]
    fn flags_nested_negative_lookahead() {
        assert_eq!(run_on(r#"const re = /(?!(?!a))/;"#).len(), 1);
    }


    #[test]
    fn ignores_tailwind_class_string() {
        assert!(run_on(r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#).is_empty());
    }


    #[test]
    fn ignores_url_string() {
        assert!(run_on(r#"const u = "http://a/b/c";"#).is_empty());
    }


    #[test]
    fn ignores_import_path() {
        assert!(run_on(r#"import X from "@scope/pkg/sub";"#).is_empty());
    }
}
