use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `.postMessage(…)` calls with only one argument (missing targetOrigin).
/// A single-argument `postMessage` is the form `obj.postMessage(data)` with no
/// comma separating a second argument inside the parens.
fn is_missing_target_origin(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find(".postMessage(") {
        let call_start = start + pos + ".postMessage(".len();

        // Walk forward tracking paren depth to find the matching close paren.
        let mut depth: i32 = 1;
        let mut has_top_level_comma = false;
        let mut in_single = false;
        let mut in_double = false;
        let mut in_backtick = false;
        let mut prev = '\0';
        let mut found_close = false;

        for ch in line[call_start..].chars() {
            match ch {
                '\'' if !in_double && !in_backtick && prev != '\\' => in_single = !in_single,
                '"' if !in_single && !in_backtick && prev != '\\' => in_double = !in_double,
                '`' if !in_single && !in_double && prev != '\\' => in_backtick = !in_backtick,
                '(' if !in_single && !in_double && !in_backtick => depth += 1,
                ')' if !in_single && !in_double && !in_backtick => {
                    depth -= 1;
                    if depth == 0 {
                        found_close = true;
                        break;
                    }
                }
                ',' if !in_single && !in_double && !in_backtick && depth == 1 => {
                    has_top_level_comma = true;
                }
                _ => {}
            }
            prev = ch;
        }

        // If we found a complete call with exactly one argument (no top-level comma)
        // and the argument area is non-empty, flag it.
        if found_close && !has_top_level_comma {
            let arg_text = &line[call_start..].trim_start();
            // Make sure there's actually an argument (not just `postMessage()`)
            if !arg_text.starts_with(')') {
                return true;
            }
        }

        start = call_start;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_missing_target_origin(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "require-post-message-target-origin".into(),
                    message: "`postMessage()` called without `targetOrigin` — provide an explicit origin."
                        .into(),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_single_arg_post_message() {
        let code = r#"window.postMessage(data);"#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_self_post_message() {
        let code = r#"self.postMessage(message);"#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_post_message_with_origin() {
        let code = r#"window.postMessage(data, "https://example.com");"#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_post_message_with_star() {
        let code = r#"window.postMessage(data, '*');"#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_nested_parens_in_first_arg() {
        // The nested parens should not confuse the comma detection
        let code = r#"window.postMessage(getData(), origin);"#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn flags_nested_call_single_arg() {
        let code = r#"window.postMessage(getData());"#;
        assert_eq!(run(code).len(), 1);
    }
}
