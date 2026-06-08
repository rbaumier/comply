//! regex-no-super-linear-move OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Detects quantifiers that can cause quadratic runtime. A quantifier
/// followed by the same literal character it matches forces re-scanning
/// on backtrack.
fn has_super_linear_move(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 1 < len {
        if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' {
            let ch = bytes[i];
            if i + 1 < len && (bytes[i + 1] == b'+' || bytes[i + 1] == b'*') {
                let after_quant = i + 2;
                let check_pos = if after_quant < len && bytes[after_quant] == b'?' {
                    after_quant + 1
                } else {
                    after_quant
                };
                if check_pos < len && bytes[check_pos] == ch {
                    return true;
                }
            }
        }
        i += 1;
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
        let AstKind::RegExpLiteral(re) = node.kind() else { return };

        let pattern = re.regex.pattern.text.as_str();
        if !has_super_linear_move(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Quantifier followed by the same element can cause quadratic runtime.".into(),
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
    fn flags_plus_followed_by_same() {
        assert_eq!(run_on(r#"const re = /a+a/;"#).len(), 1);
    }


    #[test]
    fn flags_star_followed_by_same() {
        assert_eq!(run_on(r#"const re = /a*a/;"#).len(), 1);
    }


    #[test]
    fn allows_different_char_after_quantifier() {
        assert!(run_on(r#"const re = /a+b/;"#).is_empty());
    }


    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "grid-cols-[a+a_1fr]";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://a+a.example.com";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@a+a/pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
