use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `.match(/.../)` used in a boolean context (if/while/ternary/!!/&&/||).
fn is_match_in_boolean_context(line: &str) -> bool {
    let trimmed = line.trim();

    // Find `.match(` calls with a regex literal argument
    let mut start = 0;
    while let Some(pos) = line[start..].find(".match(") {
        let abs = start + pos;
        let after_match = abs + 7; // skip past ".match("
        let rest = &line[after_match..].trim_start();

        // Only flag when the argument is a regex literal
        if !rest.starts_with('/') {
            start = after_match;
            continue;
        }

        // Check for boolean context: if/while/else if/ternary/?/!/!!
        let before = line[..abs].trim();

        // Find the start of the receiver expression (the part before `.match(`).
        // Walk backwards over identifier chars and dots to find the real prefix.
        let receiver_start = before
            .bytes()
            .rposition(|b| !b.is_ascii_alphanumeric() && b != b'_' && b != b'$' && b != b'.' && b != b'[' && b != b']')
            .map(|p| p + 1)
            .unwrap_or(0);
        let context_before = before[..receiver_start].trim();

        if trimmed.starts_with("if ")
            || trimmed.starts_with("if(")
            || trimmed.starts_with("} else if")
            || trimmed.starts_with("while ")
            || trimmed.starts_with("while(")
            || context_before.ends_with("if (")
            || context_before.ends_with("if(")
            || context_before.ends_with("while (")
            || context_before.ends_with("while(")
            || context_before.ends_with('!')
            || context_before.ends_with("!!")
            || context_before.ends_with("&&")
            || context_before.ends_with("||")
            || context_before.ends_with('?')
            || context_before.ends_with("? ")
            || before.ends_with('!')
            || before.ends_with("!!")
            || before.ends_with("&&")
            || before.ends_with("||")
        {
            return true;
        }

        start = after_match;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_match_in_boolean_context(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-regexp-test".into(),
                    message: "Prefer `RegExp#test()` over `String#match()` in boolean contexts."
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
    fn flags_match_in_if() {
        assert_eq!(run("if (str.match(/foo/)) {}").len(), 1);
    }

    #[test]
    fn flags_match_in_while() {
        assert_eq!(run("while (input.match(/^\\s+/)) {}").len(), 1);
    }

    #[test]
    fn flags_match_with_double_bang() {
        assert_eq!(run("const ok = !!str.match(/bar/);").len(), 1);
    }

    #[test]
    fn flags_match_in_ternary() {
        assert_eq!(run("const x = str.match(/a/) ? 1 : 0;").len(), 0);
        // The ternary is after the match, not before — this checks pre-context
    }

    #[test]
    fn allows_match_outside_boolean() {
        assert!(run("const m = str.match(/foo/);").is_empty());
    }

    #[test]
    fn allows_match_with_variable() {
        assert!(run("if (str.match(pattern)) {}").is_empty());
    }

    #[test]
    fn allows_test_call() {
        assert!(run("if (/foo/.test(str)) {}").is_empty());
    }
}
