//! regex-no-optional-assertion OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// Scans a regex pattern for assertions (`^`, `$`, `(?=...)`, `(?!...)`,
/// `(?<=...)`, `(?<!...)`) inside a group whose quantifier is `?` or `*`
/// (i.e. the group may match zero times, making the assertion a no-op).
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
                        if depth == 1 {
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
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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


    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        // Tailwind arbitrary-value classes contain `(` and `)?` sequences
        // that the old text scanner would flag.
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr] (^foo)?";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_url_in_string() {
        // URLs with query params can produce `(^...)?`-looking substrings.
        let src = r#"const u = "https://example.com/x?y=(^a)?";"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_empty_scoped_import_path() {
        let src = r#"import X from "";"#;
        assert!(run_on(src).is_empty());
    }
}
