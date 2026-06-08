use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn has_useless_reluctant(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let n = bytes.len();
    if n < 2 {
        return false;
    }
    for i in 0..n {
        let q = bytes[i];
        if (q == b'*' || q == b'+' || q == b'?')
            && i + 1 < n
            && bytes[i + 1] == b'?'
            && (i > 0 && bytes[i - 1] != b'\\')
        {
            let after_idx = i + 2;
            if after_idx >= n {
                return true;
            }
            let next = bytes[after_idx];
            if next == b'$' || next == b')' {
                return true;
            }
        }
    }
    false
}

/// Extract the pattern from a regex literal's `raw` field (e.g. `/foo|bar/g` -> `foo|bar`).
fn extract_pattern(raw: &str) -> Option<&str> {
    let s = raw.strip_prefix('/')?;
    let last_slash = s.rfind('/')?;
    Some(&s[..last_slash])
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
        let AstKind::RegExpLiteral(re) = node.kind() else {
            return;
        };

        let Some(raw) = &re.raw else { return };
        let Some(pattern) = extract_pattern(raw.as_str()) else {
            return;
        };

        if !has_useless_reluctant(pattern) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message:
                "Reluctant quantifier before end-of-pattern is useless \u{2014} it always matches the minimum."
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
    fn flags_reluctant_star_before_dollar() {
        assert_eq!(run_on("const re = /a*?$/;").len(), 1);
    }


    #[test]
    fn flags_reluctant_plus_before_close_paren() {
        assert_eq!(run_on("const re = /(?:a+?)/;").len(), 1);
    }


    #[test]
    fn flags_reluctant_question_before_end() {
        assert_eq!(run_on("const re = /x??/;").len(), 1);
    }


    #[test]
    fn allows_reluctant_followed_by_content() {
        assert!(run_on("const re = /a*?b/;").is_empty());
    }


    #[test]
    fn allows_greedy_before_dollar() {
        assert!(run_on("const re = /a*$/;").is_empty());
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
