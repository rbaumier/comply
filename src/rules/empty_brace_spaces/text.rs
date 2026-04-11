//! empty-brace-spaces text backend — flag `{  }` (spaces inside empty braces).
//!
//! Matches `{ }`, `{  }`, `{   }`, etc. and flags them. The fix is `{}`.
//! Does not flag braces that contain any non-whitespace content.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

/// Matches `{` followed by one or more whitespace characters then `}`.
/// Does NOT match `{}` (no space) which is already correct.
static RE_EMPTY_BRACE_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\s+\}").unwrap());

/// Returns true if the match position is likely inside a string or comment.
fn likely_in_string_or_comment(line: &str, match_start: usize) -> bool {
    let prefix = &line[..match_start];
    if prefix.contains("//") {
        return true;
    }
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut prev_backslash = false;
    for ch in prefix.chars() {
        if prev_backslash {
            prev_backslash = false;
            continue;
        }
        if ch == '\\' {
            prev_backslash = true;
            continue;
        }
        match ch {
            '\'' if !in_double && !in_backtick => in_single = !in_single,
            '"' if !in_single && !in_backtick => in_double = !in_double,
            '`' if !in_single && !in_double => in_backtick = !in_backtick,
            _ => {}
        }
    }
    in_single || in_double || in_backtick
}

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
                continue;
            }

            for mat in RE_EMPTY_BRACE_SPACES.find_iter(line) {
                if likely_in_string_or_comment(line, mat.start()) {
                    continue;
                }

                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: mat.start() + 1,
                    rule_id: "empty-brace-spaces".into(),
                    message: format!(
                        "Do not add spaces between braces: `{}` -> `{{}}`.",
                        mat.as_str()
                    ),
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
    fn flags_single_space() {
        let d = run("const obj = { };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("{}"));
    }

    #[test]
    fn flags_multiple_spaces() {
        let d = run("class Foo {   }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_tab_space() {
        let d = run("const obj = {\t};");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_empty_braces_no_space() {
        assert!(run("const obj = {};").is_empty());
    }

    #[test]
    fn allows_braces_with_content() {
        assert!(run("const obj = { a: 1 };").is_empty());
    }

    #[test]
    fn ignores_strings() {
        assert!(run(r#"const s = "{ }";"#).is_empty());
    }

    #[test]
    fn ignores_comments() {
        assert!(run("// const obj = { };").is_empty());
    }

    #[test]
    fn flags_multiple_on_one_line() {
        let d = run("const a = { }; const b = {  };");
        assert_eq!(d.len(), 2);
    }
}
