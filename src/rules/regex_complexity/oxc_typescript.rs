use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn complexity_score(pattern: &str) -> usize {
    let mut score = 0;
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                if i + 1 < bytes.len() && matches!(bytes[i + 1], b'b' | b'B') {
                    score += 1;
                }
                i += 2;
                continue;
            }
            b'*' | b'+' | b'?' | b'{' | b'|' | b'(' | b'[' | b'^' | b'$' => score += 1,
            _ => {}
        }
        i += 1;
    }
    score
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
        let threshold = ctx.config.threshold("regex-complexity", "max", ctx.lang);
        let score = complexity_score(pattern);
        if score <= threshold {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Regex complexity score is {score} (threshold: {threshold}) \u{2014} consider breaking it into smaller patterns."
            ),
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
    fn flags_complex_regex() {
        let complex =
            r#"const re = /^(a+|b*|c?)(d{2,3})(e|f|g|h)(i+|j*)(k?|l{1})(m|n|o)(p+|q*)(r?)/;"#;
        assert_eq!(run_on(complex).len(), 1);
    }


    #[test]
    fn allows_simple_regex() {
        assert!(run_on(r#"const re = /^hello$/;"#).is_empty());
    }


    #[test]
    fn allows_moderate_regex() {
        assert!(run_on(r#"const re = /\d{3}-\d{4}/;"#).is_empty());
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
