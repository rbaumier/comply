use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Control-flow keywords that introduce blocks where function declarations
/// are problematic.
const CONTROL_FLOW: &[&str] = &["if", "else", "for", "while", "switch"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let mut depth: i32 = 0;
        // Track whether we are inside a control-flow block at each depth.
        // `cf_depth_stack[d]` is true when depth `d` was opened by a
        // control-flow statement.
        let mut cf_depths: Vec<bool> = Vec::new();

        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }

            // Before processing braces on this line, check if this line starts
            // a control-flow block.
            let is_cf_line = CONTROL_FLOW.iter().any(|kw| {
                line_starts_control_flow(trimmed, kw)
            });

            for ch in line.chars() {
                if ch == '{' {
                    depth += 1;
                    let d = depth as usize;
                    if d >= cf_depths.len() {
                        cf_depths.resize(d + 1, false);
                    }
                    // Mark this depth as control-flow if the line introduced it
                    cf_depths[d] = is_cf_line || (d > 0 && cf_depths.get(d.wrapping_sub(1)).copied().unwrap_or(false) && !has_function_keyword(trimmed));
                } else if ch == '}' {
                    if depth > 0 {
                        let d = depth as usize;
                        if d < cf_depths.len() {
                            cf_depths[d] = false;
                        }
                        depth -= 1;
                    }
                }
            }

            // Now check: is there a function declaration on this line at depth > 0
            // inside a control-flow block?
            if depth > 0 && has_function_declaration(trimmed) {
                let in_cf = (1..=depth as usize).any(|d| {
                    cf_depths.get(d).copied().unwrap_or(false)
                });
                if in_cf {
                    let col = line.find("function").unwrap_or(0);
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: col + 1,
                        rule_id: "no-function-declaration-in-block".into(),
                        message: "Function declaration inside a control-flow block — move it to the top level or use a function expression.".into(),
                        severity: Severity::Error,
                    });
                }
            }
        }
        diagnostics
    }
}

fn line_starts_control_flow(trimmed: &str, keyword: &str) -> bool {
    if !trimmed.starts_with(keyword) {
        // Also handle `} else {`
        if keyword == "else" {
            return trimmed.starts_with("} else") || trimmed.starts_with("else");
        }
        return false;
    }
    let after = keyword.len();
    if after >= trimmed.len() {
        return false;
    }
    let next = trimmed.as_bytes()[after];
    // `if(`, `if `, `for(`, `while `, etc.
    next == b'(' || next == b' ' || next == b'\t'
}

/// Detect `function foo(` — a declaration, not `function(` (expression) or
/// arrow functions.
fn has_function_declaration(trimmed: &str) -> bool {
    if let Some(pos) = trimmed.find("function") {
        // Must not be preceded by alphanumeric (part of another word)
        if pos > 0 && trimmed.as_bytes()[pos - 1].is_ascii_alphanumeric() {
            return false;
        }
        let after = &trimmed[pos + 8..].trim_start();
        // Must be followed by an identifier (not `(` which would be expression)
        // and not `*` (generator, also okay to flag)
        if let Some(first) = after.chars().next() {
            return first.is_ascii_alphabetic() || first == '_' || first == '$';
        }
    }
    false
}

fn has_function_keyword(trimmed: &str) -> bool {
    trimmed.contains("function")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_function_in_if_block() {
        let src = "if (true) {\n  function foo() {}\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_function_in_for_block() {
        let src = "for (let i = 0; i < 10; i++) {\n  function bar() { return i; }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_top_level_function() {
        let src = "function baz() {\n  return 1;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arrow_in_block() {
        let src = "if (true) {\n  const fn = () => {};\n}";
        assert!(run(src).is_empty());
    }
}
