use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Scan a regex pattern for `{n,n}` quantifiers where both numbers
/// are equal (redundant — equivalent to `{n}`).
fn has_useless_two_nums_quantifier(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b'{' {
            let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
            if backslashes % 2 != 0 {
                i += 1;
                continue;
            }
            let num1_start = i + 1;
            let mut j = num1_start;
            while j < len && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > num1_start && j < len && bytes[j] == b',' {
                let num1 = &pattern[num1_start..j];
                let num2_start = j + 1;
                let mut k = num2_start;
                while k < len && bytes[k].is_ascii_digit() {
                    k += 1;
                }
                if k > num2_start && k < len && bytes[k] == b'}' {
                    let num2 = &pattern[num2_start..k];
                    if num1 == num2 {
                        return true;
                    }
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
        if !has_useless_two_nums_quantifier(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Redundant quantifier `{n,n}` \u{2014} simplify to `{n}`.".into(),
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
    fn flags_same_min_max() {
        assert_eq!(run_on("const re = /a{3,3}/;").len(), 1);
    }

    #[test]
    fn flags_same_min_max_large() {
        assert_eq!(run_on("const re = /x{10,10}/;").len(), 1);
    }

    #[test]
    fn allows_different_min_max() {
        assert!(run_on("const re = /a{1,3}/;").is_empty());
    }

    #[test]
    fn allows_single_quantifier() {
        assert!(run_on("const re = /a{3}/;").is_empty());
    }

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "grid-cols-[minmax(3,3),1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/a{3,3}/b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_empty() {
        let src = r#"import X from "@scope/pkg";"#;
        assert!(run_on(src).is_empty());
    }
}
