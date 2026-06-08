use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TemplateLiteral(template) = node.kind() else { return };
        let text = &ctx.source[template.span.start as usize..template.span.end as usize];

        if has_bad_template_escape(text) {
            let (line, column) = byte_offset_to_line_col(ctx.source, template.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Use `\\${` instead of `$\\{` to escape in template literals.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Check if template text contains `$\{` or `\$\{` (bad escapes).
fn has_bad_template_escape(text: &str) -> bool {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        let b = bytes[i];

        // Skip real interpolations `${...}`
        if b == b'$' && i + 1 < len && bytes[i + 1] == b'{' {
            i += 2;
            let mut depth = 1i32;
            while i < len && depth > 0 {
                if bytes[i] == b'{' {
                    depth += 1;
                } else if bytes[i] == b'}' {
                    depth -= 1;
                } else if bytes[i] == b'\\' && i + 1 < len {
                    i += 1;
                }
                i += 1;
            }
            continue;
        }

        // `\$\{` -- backslash-dollar-backslash-brace (bad: escapes both)
        if b == b'\\'
            && i + 3 < len
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'\\'
            && bytes[i + 3] == b'{'
            && !is_preceded_by_odd_backslashes(bytes, i)
        {
            return true;
        }

        // `$\{` -- dollar-backslash-brace (bad: escapes only the brace)
        if b == b'$'
            && i + 2 < len
            && bytes[i + 1] == b'\\'
            && bytes[i + 2] == b'{'
            && !is_preceded_by_odd_backslashes(bytes, i)
        {
            return true;
        }

        // `\${` -- correct pattern, skip past it
        if b == b'\\'
            && i + 2 < len
            && bytes[i + 1] == b'$'
            && bytes[i + 2] == b'{'
            && !is_preceded_by_odd_backslashes(bytes, i)
        {
            i += 3;
            continue;
        }

        // Skip other escape sequences
        if b == b'\\' && i + 1 < len {
            i += 2;
            continue;
        }

        i += 1;
    }

    false
}

fn is_preceded_by_odd_backslashes(bytes: &[u8], pos: usize) -> bool {
    let mut count = 0;
    let mut p = pos;
    while p > 0 && bytes[p - 1] == b'\\' {
        count += 1;
        p -= 1;
    }
    count % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_dollar_backslash_brace() {
        let d = run_on(r#"const s = `$\{foo}`;"#);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_escaped_dollar_and_brace() {
        let d = run_on(r#"const s = `\$\{foo}`;"#);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_backslash_dollar_brace() {
        assert!(run_on(r#"const s = `\${foo}`;"#).is_empty());
    }


    #[test]
    fn allows_normal_interpolation() {
        assert!(run_on(r#"const s = `${foo}`;"#).is_empty());
    }


    #[test]
    fn allows_plain_template() {
        assert!(run_on(r#"const s = `hello world`;"#).is_empty());
    }
}
