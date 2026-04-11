use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract a variable name from a top-level `let` or `var` declaration.
/// Only matches lines at indent level 0.
fn top_level_mutable_var(line: &str) -> Option<&str> {
    // Must start at column 0 (no leading whitespace)
    if line.starts_with(' ') || line.starts_with('\t') {
        return None;
    }
    let rest = line.strip_prefix("let ").or_else(|| line.strip_prefix("var "))?;
    let rest = rest.trim_start();
    // variable name ends at whitespace, `=`, `:`, `;`, or `,`
    let end = rest.find(|c: char| c.is_whitespace() || c == '=' || c == ':' || c == ';' || c == ',')?;
    if end == 0 {
        return None;
    }
    Some(&rest[..end])
}

/// Detect function declarations/expressions and return (line_index, name, body_start_index).
/// We track brace depth to find the full function body.
struct FuncSpan {
    name: String,
    decl_line: usize,
    start: usize, // line index of opening brace
    end: usize,   // line index of closing brace (inclusive)
}

fn find_functions(source: &str) -> Vec<FuncSpan> {
    let lines: Vec<&str> = source.lines().collect();
    let mut functions = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        // Match: `function name(`, `async function name(`, `export function name(`,
        // `export async function name(`
        let func_name = extract_function_name(trimmed);
        if let Some(name) = func_name {
            // Find the opening brace
            let mut brace_line = i;
            while brace_line < lines.len() && !lines[brace_line].contains('{') {
                brace_line += 1;
            }
            if brace_line < lines.len() {
                // Count braces to find the end
                let mut depth = 0i32;
                let mut end_line = brace_line;
                for (j, &line) in lines.iter().enumerate().skip(brace_line) {
                    for ch in line.chars() {
                        if ch == '{' {
                            depth += 1;
                        } else if ch == '}' {
                            depth -= 1;
                        }
                    }
                    if depth <= 0 {
                        end_line = j;
                        break;
                    }
                }
                functions.push(FuncSpan {
                    name: name.to_string(),
                    decl_line: i,
                    start: brace_line,
                    end: end_line,
                });
                i = end_line + 1;
                continue;
            }
        }
        i += 1;
    }

    functions
}

fn extract_function_name(trimmed: &str) -> Option<&str> {
    let rest = trimmed
        .strip_prefix("export ")
        .unwrap_or(trimmed);
    let rest = rest
        .strip_prefix("async ")
        .unwrap_or(rest);
    let rest = rest.strip_prefix("function ")?;
    let rest = rest.trim_start();
    let end = rest.find(|c: char| c == '(' || c == '<' || c.is_whitespace())?;
    if end == 0 {
        return None;
    }
    Some(&rest[..end])
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let lines: Vec<&str> = ctx.source.lines().collect();

        // 1. Collect top-level mutable variable names
        let mutable_vars: Vec<&str> = lines
            .iter()
            .filter_map(|line| top_level_mutable_var(line))
            .collect();

        if mutable_vars.is_empty() {
            return Vec::new();
        }

        // 2. Find functions and check if their bodies reference any mutable var
        let functions = find_functions(ctx.source);
        let mut diagnostics = Vec::new();

        for func in &functions {
            let body_lines = &lines[func.start..=func.end.min(lines.len() - 1)];
            for var in &mutable_vars {
                let referenced = body_lines.iter().any(|line| {
                    // Check for whole-word match using simple boundary detection
                    let mut search_start = 0;
                    while let Some(pos) = line[search_start..].find(var) {
                        let abs = search_start + pos;
                        let before_ok = abs == 0
                            || !line.as_bytes()[abs - 1].is_ascii_alphanumeric()
                                && line.as_bytes()[abs - 1] != b'_';
                        let after = abs + var.len();
                        let after_ok = after >= line.len()
                            || !line.as_bytes()[after].is_ascii_alphanumeric()
                                && line.as_bytes()[after] != b'_';
                        if before_ok && after_ok {
                            return true;
                        }
                        search_start = abs + 1;
                    }
                    false
                });
                if referenced {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: func.decl_line + 1,
                        column: 1,
                        rule_id: "pure-by-default".into(),
                        message: format!(
                            "Function `{}` references mutable top-level state `{}`.",
                            func.name, var,
                        ),
                        severity: Severity::Warning,
                    });
                    break; // one diagnostic per function is enough
                }
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
    fn flags_function_using_top_level_let() {
        let src = "\
let counter = 0;

function increment() {
    counter += 1;
}
";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("increment"));
        assert!(d[0].message.contains("counter"));
    }

    #[test]
    fn allows_function_without_top_level_state() {
        let src = "\
const MAX = 100;

function add(a: number, b: number) {
    return a + b;
}
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_var_at_top_level() {
        let src = "\
var state = {};

function reset() {
    state = {};
}
";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("reset"));
    }

    #[test]
    fn ignores_let_inside_function() {
        let src = "\
function foo() {
    let x = 1;
    return x;
}
";
        assert!(run(src).is_empty());
    }
}
