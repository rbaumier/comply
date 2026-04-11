use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `function` keyword declarations (not arrow functions).
/// Returns `true` if the line contains a function declaration keyword.
fn is_function_decl(line: &str) -> bool {
    let trimmed = line.trim();
    // Match: `function foo(`, `async function foo(`, `export function`,
    // `export default function`, or standalone `function(` (function expression assigned).
    // Skip lines that are just comments.
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
        return false;
    }
    // Look for `function ` or `function(` as a keyword boundary.
    let mut search = line.as_bytes();
    while let Some(pos) = find_subsequence(search, b"function") {
        let after = pos + 8;
        if after < search.len() {
            let next = search[after];
            // `function ` or `function(` or `function*`
            if next == b' ' || next == b'(' || next == b'*' {
                // Make sure it's not part of a larger word (e.g. `dysfunction`)
                if pos == 0 || !search[pos - 1].is_ascii_alphanumeric() {
                    return true;
                }
            }
        }
        search = &search[after..];
    }
    false
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut fn_depth: usize = 0;

        for (idx, line) in ctx.source.lines().enumerate() {
            let has_fn = is_function_decl(line);

            if has_fn {
                if fn_depth >= 2 {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: "no-nested-functions".into(),
                        message: format!(
                            "Function declared at nesting depth {} — extract to module scope.",
                            fn_depth
                        ),
                        severity: Severity::Warning,
                    });
                }
                fn_depth += 1;
            }

            // Track brace depth to know when we leave a function scope.
            // This is a rough heuristic — good enough for text-based checks.
            for ch in line.chars() {
                if ch == '{' {
                    // Already counted via fn_depth for function lines.
                } else if ch == '}' {
                    if fn_depth > 0 {
                        fn_depth = fn_depth.saturating_sub(0);
                        // We can't perfectly track which `}` closes which function,
                        // but we decrement on net-close.
                    }
                }
            }

            // Simple heuristic: count open/close braces to track depth.
            // When a function's braces fully close, depth goes down.
            let opens = line.chars().filter(|&c| c == '{').count();
            let closes = line.chars().filter(|&c| c == '}').count();
            if closes > opens && fn_depth > 0 {
                fn_depth -= 1;
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
    fn flags_deeply_nested_function() {
        let src = r#"function outer() {
  function middle() {
    function tooDeep() {
      return 1;
    }
  }
}"#;
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-nested-functions");
        assert!(d[0].message.contains("depth 2"));
    }

    #[test]
    fn allows_two_levels() {
        let src = r#"function outer() {
  function inner() {
    return 1;
  }
}"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_top_level_function() {
        let src = r#"function foo() {
  return 1;
}"#;
        assert!(run(src).is_empty());
    }
}
