//! regex-no-optional-assertion OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::regex_helpers::is_inside_char_class;
use std::sync::Arc;

/// Scans a regex pattern for assertions (`^`, `$`, `(?=...)`, `(?!...)`,
/// `(?<=...)`, `(?<!...)`) inside a group whose quantifier is `?` or `*`
/// (i.e. the group may match zero times, making the assertion a no-op).
///
/// `^` / `$` inside a `[...]` character class are literal members (or the
/// negation marker for a leading `^`), never positional assertions, so they
/// are not counted.
fn has_optional_assertion(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'(' {
            let mut depth = 1;
            let mut j = i + 1;
            let mut has_assertion = false;
            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => j += 1,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'^' | b'$' => {
                        if depth == 1 && !is_inside_char_class(bytes, j) {
                            has_assertion = true;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            // Check for lookaround `(?=...)`, `(?!...)`, `(?<=...)`, `(?<!...)`
            // anywhere inside the group.
            if !has_assertion {
                let mut k = i + 1;
                while k + 2 < j {
                    if bytes[k] == b'(' && bytes[k + 1] == b'?' {
                        let c = bytes[k + 2];
                        if c == b'=' || c == b'!' {
                            has_assertion = true;
                            break;
                        }
                        if c == b'<' && k + 3 < j {
                            let d = bytes[k + 3];
                            if d == b'=' || d == b'!' {
                                has_assertion = true;
                                break;
                            }
                        }
                    }
                    k += 1;
                }
            }
            if depth == 0 && has_assertion && j + 1 < len {
                let next = bytes[j + 1];
                if next == b'?' || next == b'*' {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

pub struct Check;

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
        if !has_optional_assertion(pattern) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, regexp.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Assertion inside an optional group is effectively ignored.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_assertion_in_optional_group() {
        assert_eq!(run_on(r#"const re = /(?:^foo)?bar/;"#).len(), 1);
    }

    #[test]
    fn allows_assertion_in_required_group() {
        assert!(run_on(r#"const re = /(?:^foo)bar/;"#).is_empty());
    }

    #[test]
    fn flags_assertion_in_star_group() {
        assert_eq!(run_on(r#"const re = /(?:^foo)*bar/;"#).len(), 1);
    }

    // --- Char-class `^` / `$` regression tests (issue #3887). ---

    #[test]
    fn allows_negated_char_class_caret_in_star_group() {
        // The `^` in `[^"\\]` is the negation marker, not a start anchor.
        assert!(run_on(r#"const r1 = /(?:\\.[^"\\]*)*/g;"#).is_empty());
    }

    #[test]
    fn allows_negated_char_class_caret_in_alternation() {
        // The `^` in `[^']` is the negation marker, not a start anchor.
        assert!(run_on(r#"const r2 = /(?:''|[^'])*/;"#).is_empty());
    }

    #[test]
    fn allows_typeorm_postgres_hstore_pattern() {
        let src = r#"const re = /"([^"\\]*(?:\\.[^"\\]*)*)"=>(?:(NULL)|"([^"\\]*(?:\\.[^"\\]*)*)")(?:,|$)/g;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_typeorm_sqlite_json_pattern() {
        let src = r#"const re = /^(jsonb|json)\s*\(\s*'((?:''|[^'])*)'\s*\)\s*$/i;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_real_start_anchor_in_star_group() {
        // `^` outside any char class, inside a `*`-quantified group: a real no-op.
        assert_eq!(run_on(r#"const r3 = /(?:^abc)*/;"#).len(), 1);
    }

    #[test]
    fn still_flags_real_end_anchor_in_optional_group() {
        // `$` outside any char class, inside a `?`-quantified group: a real no-op.
        assert_eq!(run_on(r#"const re = /(?:abc$)?/;"#).len(), 1);
    }
}
