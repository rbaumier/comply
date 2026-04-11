use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn extract_params(lines: &[&str], fn_line_idx: usize) -> Vec<String> {
    let mut combined = String::new();
    for line in &lines[fn_line_idx..] {
        combined.push_str(line);
        combined.push(' ');
        if combined.contains(')') {
            break;
        }
    }

    let open = match combined.find('(') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let close = match combined[open..].find(')') {
        Some(i) => open + i,
        None => return Vec::new(),
    };
    let param_str = &combined[open + 1..close];

    let mut params = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();

    for ch in param_str.chars() {
        match ch {
            '<' | '(' | '{' | '[' => {
                depth += 1;
                current.push(ch);
            }
            '>' | ')' | '}' | ']' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                let name = extract_param_name(&current);
                if !name.is_empty() {
                    params.push(name);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let name = extract_param_name(&current);
    if !name.is_empty() {
        params.push(name);
    }
    params
}

fn extract_param_name(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.starts_with('{') || trimmed.starts_with('[') {
        return String::new();
    }
    let trimmed = trimmed.strip_prefix("...").unwrap_or(trimmed);
    let modifiers = ["public ", "private ", "protected ", "readonly "];
    let mut trimmed = trimmed;
    for m in &modifiers {
        if let Some(rest) = trimmed.strip_prefix(m) {
            trimmed = rest;
        }
    }
    let name: String = trimmed
        .chars()
        .take_while(|&c| c != ':' && c != '=' && c != '?')
        .collect();
    name.trim().to_string()
}

fn has_param_tag(lines: &[&str], start: usize, end: usize, name: &str) -> bool {
    for line in &lines[start..=end] {
        let content = line.trim().trim_start_matches('*').trim();
        if let Some(after) = content.strip_prefix("@param") {
            let after = after.trim_start();
            let after = if let Some(rest) = after.strip_prefix('{') {
                match rest.find('}') {
                    Some(i) => rest[i + 1..].trim_start(),
                    None => after,
                }
            } else {
                after
            };
            let param_name: String = after
                .chars()
                .take_while(|&c| c.is_alphanumeric() || c == '_' || c == '$')
                .collect();
            if param_name == name {
                return true;
            }
        }
    }
    false
}

fn find_function_line(lines: &[&str], block_end: usize) -> Option<usize> {
    for (i, line) in lines.iter().enumerate().skip(block_end + 1).take(3) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('@') || trimmed.starts_with("//") {
            continue;
        }
        let stripped = trimmed
            .strip_prefix("export ")
            .map(|s| s.strip_prefix("default ").unwrap_or(s))
            .map(|s| s.strip_prefix("async ").unwrap_or(s))
            .unwrap_or(trimmed);
        if stripped.starts_with("function ") || stripped.contains('(') {
            return Some(i);
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let trimmed = lines[i].trim();
            if !trimmed.starts_with("/**") {
                i += 1;
                continue;
            }

            let block_start = i;
            let mut block_end = i;
            if !trimmed.contains("*/") || trimmed == "/**" {
                let mut j = i + 1;
                while j < lines.len() {
                    if lines[j].trim().contains("*/") {
                        block_end = j;
                        break;
                    }
                    j += 1;
                }
            }

            if let Some(fn_line) = find_function_line(&lines, block_end) {
                let actual_params = extract_params(&lines, fn_line);
                for param in &actual_params {
                    if !has_param_tag(&lines, block_start, block_end, param) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: block_start + 1,
                            column: 1,
                            rule_id: "jsdoc-require-param".into(),
                            message: format!(
                                "JSDoc is missing `@param {param}`. Document every parameter so callers understand the API."
                            ),
                            severity: Severity::Warning,
                        });
                    }
                }
            }

            i = block_end + 1;
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
    fn flags_missing_param_doc() {
        let source = r#"
/**
 * Greets a user.
 */
function greet(name: string) {}
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("name"));
    }

    #[test]
    fn allows_fully_documented_params() {
        let source = r#"
/**
 * Adds two numbers.
 * @param a - first
 * @param b - second
 */
function add(a: number, b: number) { return a + b; }
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_partially_documented() {
        let source = r#"
/**
 * Process.
 * @param a - first
 */
function process(a: number, b: number) {}
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("b"));
    }
}
