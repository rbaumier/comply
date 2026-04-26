//! consistent-template-literal-escape — flag `$\{` or `\$\{` inside
//! template literals. The correct escape is `\${`.
//!
//! Walks template_string nodes and inspects their raw text for bad
//! escape patterns.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["template_string"] => |node, source, ctx, diagnostics|
    let text = match node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };

    if has_bad_template_escape(text) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "consistent-template-literal-escape".into(),
            message: "Use `\\${` instead of `$\\{` to escape in template literals.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Check if template text contains `$\{` or `\$\{` (bad escapes).
///
/// We scan the raw bytes looking for:
///   - `$\{` (dollar then backslash-brace) -- bad
///   - `\$\{` (backslash-dollar then backslash-brace) -- bad
///
/// We must NOT flag `\${` which is the correct escape.
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
        crate::rules::test_helpers::run_ts(source, &Check)
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
