//! OxcCheck backend for regex-no-useless-quantifier.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::regex_helpers::is_inside_char_class;
use std::sync::Arc;

pub struct Check;

/// Detects useless quantifiers inside a regex pattern:
/// - `{1}` — matches exactly once anyway
/// - `{1,1}` — same
/// - Quantifier on an empty group `()+`, `()*`, `()?`, `(){...}`
///
/// Detection is skipped inside a `[...]` character class, where `(`, `)`, `{`,
/// `}` are literal members rather than group/quantifier syntax.
fn has_useless_quantifier(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Respect escapes: `\{`, `\(` etc. are literals.
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }

        // Inside `[...]`, `(`/`)`/`{`/`}` are literal members, not syntax.
        if is_inside_char_class(bytes, i) {
            i += 1;
            continue;
        }

        // Detect `{1}` or `{1,1}`.
        if bytes[i] == b'{' {
            let mut j = i + 1;
            let mut num_buf = String::new();
            while j < len && bytes[j].is_ascii_digit() {
                num_buf.push(bytes[j] as char);
                j += 1;
            }
            if j < len && bytes[j] == b'}' && num_buf == "1" {
                return true;
            } else if j < len && bytes[j] == b',' {
                j += 1;
                let mut num_buf2 = String::new();
                while j < len && bytes[j].is_ascii_digit() {
                    num_buf2.push(bytes[j] as char);
                    j += 1;
                }
                if j < len && bytes[j] == b'}' && num_buf == "1" && num_buf2 == "1" {
                    return true;
                }
            }
        }

        // Detect quantifier on empty group: ()+, ()*, ()?, (){...}.
        if bytes[i] == b'(' && i + 2 < len && bytes[i + 1] == b')' {
            let after = bytes[i + 2];
            if after == b'+' || after == b'*' || after == b'?' || after == b'{' {
                return true;
            }
        }

        i += 1;
    }
    false
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
    fn flags_redundant_quantifier_one() {
        assert_eq!(run_on(r#"const re = /a{1}/;"#).len(), 1);
    }

    #[test]
    fn flags_redundant_quantifier_one_one() {
        assert_eq!(run_on(r#"const re = /a{1,1}/;"#).len(), 1);
    }

    #[test]
    fn flags_empty_group_quantified() {
        // `(){2}` and `()*` — quantified empty group, outside any class.
        assert_eq!(run_on(r#"const re = /(){2}/;"#).len(), 1);
        assert_eq!(run_on(r#"const re = /()*/;"#).len(), 1);
    }

    #[test]
    fn allows_meaningful_quantifier() {
        assert!(run_on(r#"const re = /a{2}/;"#).is_empty());
    }

    // --- Regression: char-class context (issue #3927). ---

    #[test]
    fn allows_literal_group_chars_inside_char_class() {
        // `/[(){}]*/` — `(`, `)`, `{`, `}` are literal class members; the
        // trailing `*` legitimately quantifies the whole class.
        assert!(run_on(r#"const re = /[(){}]*/;"#).is_empty());
    }

    #[test]
    fn allows_eslint_prefer_regex_literals_class() {
        // Verbatim from eslint/eslint prefer-regex-literals.js:525 — the
        // `(){}` are literal members of the character class.
        let src = r#"const re = /^[-\w\\[\](){} \t\r\n\v\f!@#$%^&*+=/~`.><?,'"|:;]*$/u;"#;
        assert!(run_on(src).is_empty(), "literal (){{}} inside [...] must not flag");
    }

    #[test]
    fn allows_brace_one_inside_char_class() {
        // `{1}` literal members inside a class are not a redundant quantifier.
        assert!(run_on(r#"const re = /[{1}]/;"#).is_empty());
    }
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
        if !has_useless_quantifier(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Useless quantifier \u{2014} it can only match once or matches an empty element.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
