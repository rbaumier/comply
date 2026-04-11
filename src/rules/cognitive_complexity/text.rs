use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Keywords that increment cognitive complexity.
const FLOW_KEYWORDS: &[&str] = &[
    "if ", "if(", "else if ", "else if(", "else ", "else{",
    "for ", "for(", "while ", "while(", "switch ", "switch(",
    "case ", "catch ", "catch(",
];

/// Logical operators that increment cognitive complexity.
const LOGICAL_OPS: &[&str] = &["&&", "||", "??"];

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let threshold = ctx.config.threshold("cognitive-complexity", "max", 5) as u32;

        let mut i = 0;
        while i < lines.len() {
            let trimmed = lines[i].trim();

            // Detect function start: `function name(`, `async function`, arrow `=> {`,
            // or method shorthand `name(args) {`
            if is_function_start(trimmed) {
                let fn_start = i;
                let (complexity, fn_end) = compute_complexity(&lines, i);
                if complexity > threshold {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: fn_start + 1,
                        column: 1,
                        rule_id: "cognitive-complexity".into(),
                        message: format!(
                            "Cognitive complexity is {complexity} (threshold {threshold}). Simplify this function."
                        ),
                        severity: Severity::Error,
                    });
                }
                i = fn_end + 1;
            } else {
                i += 1;
            }
        }

        diagnostics
    }
}

/// Heuristic: detect lines that start a function body.
fn is_function_start(trimmed: &str) -> bool {
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
        return false;
    }
    // `function name(` or `async function`
    if trimmed.starts_with("function ") || trimmed.starts_with("async function ") {
        return true;
    }
    // `export function`, `export default function`, `export async function`
    if trimmed.starts_with("export ")
        && (trimmed.contains("function ") || trimmed.contains("function("))
    {
        return true;
    }
    // Arrow function assigned: `const/let/var name = (...) => {`  or  `name = (...) => {`
    if trimmed.contains("=> {") || trimmed.contains("=>{") {
        return true;
    }
    false
}

/// Walk the function body counting cognitive complexity.
/// Returns (complexity_score, last_line_index_of_function).
fn compute_complexity(lines: &[&str], start: usize) -> (u32, usize) {
    // Find the opening brace
    let mut brace_depth: i32 = 0;
    let mut found_open = false;
    let mut complexity: u32 = 0;
    let mut nesting: u32 = 0;
    let mut end = start;

    for (i, &line) in lines.iter().enumerate().skip(start) {
        let trimmed = line.trim();

        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
            continue;
        }

        // Count braces
        for ch in trimmed.chars() {
            if ch == '{' {
                brace_depth += 1;
                if !found_open {
                    found_open = true;
                    continue;
                }
                // Entering a nested block
                nesting += 1;
            } else if ch == '}' {
                brace_depth -= 1;
                if found_open && brace_depth == 0 {
                    end = i;
                    return (complexity, end);
                }
                if nesting > 0 {
                    nesting -= 1;
                }
            }
        }

        // Only count complexity inside the function body
        if !found_open || i == start {
            continue;
        }

        // Count flow keywords — each adds 1 + current nesting depth
        for kw in FLOW_KEYWORDS {
            if trimmed.contains(kw) {
                // Don't double-count `else if` — if we match `else if`, skip `else`
                if kw.starts_with("else if") || (!kw.starts_with("else") || !trimmed.contains("else if")) {
                    complexity += 1 + nesting.saturating_sub(1);
                }
                break; // One keyword per line max
            }
        }

        // Count logical operators (each distinct sequence adds 1)
        for op in LOGICAL_OPS {
            let count = trimmed.matches(op).count() as u32;
            complexity += count;
        }

        // Ternary `?` — count occurrences but skip `?.` (optional chaining)
        let ternary_count = count_ternary(trimmed);
        complexity += ternary_count;
    }

    (complexity, end.max(start))
}

/// Count ternary `?` operators, excluding `?.` (optional chaining) and `??`.
fn count_ternary(line: &str) -> u32 {
    let mut count = 0u32;
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'?' {
            // Skip `?.` (optional chaining)
            if i + 1 < bytes.len() && bytes[i + 1] == b'.' {
                continue;
            }
            // Skip `??` (nullish coalescing — counted separately)
            if i + 1 < bytes.len() && bytes[i + 1] == b'?' {
                continue;
            }
            // Skip second `?` of `??`
            if i > 0 && bytes[i - 1] == b'?' {
                continue;
            }
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_complex_function() {
        let src = r#"function process(items) {
  if (items.length === 0) {
    return;
  }
  for (const item of items) {
    if (item.active) {
      if (item.value > 10) {
        switch (item.type) {
          case 'a':
            break;
          case 'b':
            break;
        }
      }
    }
  }
}"#;
        let d = run(src);
        assert!(!d.is_empty(), "should flag complex function");
    }

    #[test]
    fn allows_simple_function() {
        let src = "function add(a, b) {\n  return a + b;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_moderate_function() {
        let src = r#"function check(x) {
  if (x > 0) {
    return true;
  }
  return false;
}"#;
        assert!(run(src).is_empty());
    }
}
