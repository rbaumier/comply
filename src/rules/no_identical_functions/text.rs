use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract function-like declarations and their bodies.
/// Returns `(name, start_line_0based, normalized_body)`.
fn extract_functions(source: &str) -> Vec<(String, usize, String)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut functions = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Match common TS/JS function patterns:
        //   function foo(...)  {
        //   const foo = (...) => {
        //   const foo = function(...) {
        //   foo(...) {              (method shorthand)
        let name = extract_function_name(trimmed);
        if name.is_none() {
            continue;
        }
        let name = name.unwrap();

        // Find the opening brace on this line or the next
        let open_brace_line = if trimmed.ends_with('{') {
            i
        } else if i + 1 < lines.len() && lines[i + 1].trim() == "{" {
            i + 1
        } else {
            continue;
        };

        // Extract body between braces
        if let Some(body) = extract_brace_body(&lines, open_brace_line) {
            let body_lines: Vec<&str> = body.lines().collect();
            if body_lines.len() > 3 {
                let normalized = normalize_body(&body);
                functions.push((name, i, normalized));
            }
        }
    }

    functions
}

fn extract_function_name(line: &str) -> Option<String> {
    // function foo(
    if let Some(rest) = line.strip_prefix("function ") {
        if let Some(paren) = rest.find('(') {
            let name = rest[..paren].trim();
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Some(name.to_string());
            }
        }
        return None;
    }

    // async function foo(
    if let Some(rest) = line.strip_prefix("async function ") {
        if let Some(paren) = rest.find('(') {
            let name = rest[..paren].trim();
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Some(name.to_string());
            }
        }
        return None;
    }

    // const/let/var foo = (...) => {  or  const foo = function(...) {
    for kw in &["const ", "let ", "var "] {
        if let Some(rest) = line.strip_prefix(kw) {
            if let Some(eq) = rest.find('=') {
                let name = rest[..eq].trim();
                if !name.is_empty()
                    && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && (rest[eq..].contains("=>") || rest[eq..].contains("function"))
                {
                    return Some(name.to_string());
                }
            }
        }
    }

    // Method shorthand: foo(...) {  (but not if/for/while/switch/catch)
    if let Some(paren) = line.find('(') {
        let candidate = line[..paren].trim();
        if !candidate.is_empty()
            && candidate
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_')
            && !matches!(
                candidate,
                "if" | "for" | "while" | "switch" | "catch" | "else" | "return"
            )
            && line.ends_with('{')
        {
            return Some(candidate.to_string());
        }
    }

    None
}

/// Extract the body between the opening `{` and its matching `}`.
/// `open_line` is the index of the line containing the opening brace.
fn extract_brace_body(lines: &[&str], open_line: usize) -> Option<String> {
    let mut depth: i32 = 0;
    let mut body_lines = Vec::new();
    let mut started = false;

    for line in &lines[open_line..] {
        for ch in line.chars() {
            if ch == '{' {
                depth += 1;
                started = true;
            } else if ch == '}' {
                depth -= 1;
            }
        }
        if started {
            body_lines.push(*line);
        }
        if started && depth == 0 {
            // Remove the first `{` line and the last `}` line to get pure body
            if body_lines.len() > 2 {
                return Some(body_lines[1..body_lines.len() - 1].join("\n"));
            }
            return Some(String::new());
        }
    }
    None
}

fn normalize_body(body: &str) -> String {
    body.lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let functions = extract_functions(ctx.source);

        for i in 1..functions.len() {
            for j in 0..i {
                if functions[i].2 == functions[j].2 {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: functions[i].1 + 1,
                        column: 1,
                        rule_id: "no-identical-functions".into(),
                        message: format!(
                            "Function `{}` has an identical body to `{}` (line {}). Extract the duplicated logic into a shared helper.",
                            functions[i].0,
                            functions[j].0,
                            functions[j].1 + 1,
                        ),
                        severity: Severity::Error,
                    });
                    break; // Only flag once per duplicate
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
    fn flags_identical_functions() {
        let source = r#"
function foo(x: number) {
    const a = x + 1;
    const b = a * 2;
    console.log(b);
    return b;
}

function bar(x: number) {
    const a = x + 1;
    const b = a * 2;
    console.log(b);
    return b;
}
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("bar"));
        assert!(d[0].message.contains("foo"));
    }

    #[test]
    fn allows_different_functions() {
        let source = r#"
function foo(x: number) {
    const a = x + 1;
    const b = a * 2;
    console.log(b);
    return b;
}

function bar(x: number) {
    const a = x - 1;
    const b = a / 2;
    console.log(b);
    return b;
}
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_short_identical_bodies() {
        let source = r#"
function foo() {
    return 1;
}

function bar() {
    return 1;
}
"#;
        // Bodies are <= 3 lines, so no flag
        assert!(run(source).is_empty());
    }
}
