use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// The eslint disable directives that require a rule list.
const DIRECTIVES: &[&str] = &[
    "eslint-disable-next-line",
    "eslint-disable-line",
    "eslint-disable",
];

/// Returns true if the comment contains an eslint-disable directive without
/// specifying which rule(s) to disable.
fn is_abusive_disable(line: &str) -> bool {
    for directive in DIRECTIVES {
        if let Some(pos) = line.find(directive) {
            let end = pos + directive.len();
            // If the char right after the directive is a hyphen, this is a
            // longer directive (e.g. `eslint-disable` inside `eslint-disable-next-line`).
            // Skip — the longer directive will match on its own iteration.
            if line.as_bytes().get(end) == Some(&b'-') {
                continue;
            }
            let after_trimmed = line[end..].trim();
            // Nothing after the directive (or just end-of-comment `*/`)
            if after_trimmed.is_empty() || after_trimmed == "*/" || after_trimmed == "-->" {
                return true;
            }
            // `-- reason` is eslint's description separator (no rule specified)
            if after_trimmed.starts_with("--") {
                return true;
            }
            // Rule names start with a letter or `@` (scoped packages).
            // Anything else means no rule was specified.
            if let Some(first) = after_trimmed.chars().next()
                && !first.is_ascii_alphabetic()
                && first != '@'
            {
                return true;
            }
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            // Only check comment lines
            if !trimmed.starts_with("//")
                && !trimmed.starts_with("/*")
                && !trimmed.contains("//")
                && !trimmed.contains("/*")
            {
                continue;
            }
            if is_abusive_disable(trimmed) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-abusive-eslint-disable".into(),
                    message: "Specify the rules you want to disable.".into(),
                    severity: Severity::Warning,
                    span: None,
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
    fn flags_bare_disable_next_line() {
        assert_eq!(run("// eslint-disable-next-line").len(), 1);
    }

    #[test]
    fn flags_bare_disable() {
        assert_eq!(run("/* eslint-disable */").len(), 1);
    }

    #[test]
    fn flags_bare_disable_line() {
        assert_eq!(run("const x = 1; // eslint-disable-line").len(), 1);
    }

    #[test]
    fn allows_specific_rule() {
        assert!(run("// eslint-disable-next-line no-console").is_empty());
    }

    #[test]
    fn allows_specific_rule_in_block() {
        assert!(run("/* eslint-disable no-unused-vars */").is_empty());
    }

    #[test]
    fn allows_scoped_rule() {
        assert!(run("// eslint-disable-next-line @typescript-eslint/no-explicit-any").is_empty());
    }

    #[test]
    fn flags_with_description_separator() {
        assert_eq!(run("// eslint-disable-next-line -- reason").len(), 1);
    }

    #[test]
    fn ignores_non_comment_lines() {
        assert!(run("const eslintDisable = true;").is_empty());
    }
}
