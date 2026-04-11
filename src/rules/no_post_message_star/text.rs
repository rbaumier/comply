use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects `.postMessage(…, "*")` or `.postMessage(…, '*')`.
/// The `"*"` / `'*'` must be the last argument before the closing paren.
fn has_post_message_star(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find(".postMessage(") {
        let call_start = start + pos + 13; // skip past ".postMessage("
        // Find the matching closing paren (simple: last ')' after call_start).
        if let Some(close) = line[call_start..].rfind(')') {
            let args = line[call_start..call_start + close].trim();
            // The last argument should be "*" or '*'
            if args.ends_with("\"*\"") || args.ends_with("'*'") {
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
            if has_post_message_star(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-post-message-star".into(),
                    message: "`postMessage` with `\"*\"` target origin — specify an explicit origin.".into(),
                    severity: Severity::Error,
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
    fn flags_double_quote_star() {
        assert_eq!(run(r#"window.postMessage(data, "*");"#).len(), 1);
    }

    #[test]
    fn flags_single_quote_star() {
        assert_eq!(run("iframe.contentWindow.postMessage(msg, '*');").len(), 1);
    }

    #[test]
    fn allows_specific_origin() {
        assert!(run(r#"window.postMessage(data, "https://example.com");"#).is_empty());
    }

    #[test]
    fn allows_variable_origin() {
        assert!(run("window.postMessage(data, targetOrigin);").is_empty());
    }
}
