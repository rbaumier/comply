use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Extract function parameter names from the function signature line.
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
    let trimmed = strip_ts_modifiers(trimmed);
    let name: String = trimmed
        .chars()
        .take_while(|&c| c != ':' && c != '=' && c != '?')
        .collect();
    name.trim().to_string()
}

fn strip_ts_modifiers(s: &str) -> &str {
    let modifiers = ["public ", "private ", "protected ", "readonly "];
    let mut result = s;
    for m in &modifiers {
        if let Some(rest) = result.strip_prefix(m) {
            result = rest;
        }
    }
    result
}

/// Extract `@param` names from a JSDoc block.
fn extract_jsdoc_param_names(lines: &[&str], start: usize) -> Vec<(String, usize)> {
    let mut params = Vec::new();
    for (i, line) in lines.iter().enumerate().skip(start) {
        let trimmed = line.trim();
        let content = trimmed
            .trim_start_matches("/**")
            .trim_start_matches('*')
            .trim();

        if let Some(after_param) = content.strip_prefix("@param") {
            let after_param = after_param.trim_start();
            let name_str = if let Some(rest) = after_param.strip_prefix('{') {
                match rest.find('}') {
                    Some(close) => rest[close + 1..].trim_start(),
                    None => after_param,
                }
            } else {
                after_param
            };
            let param_name: String = name_str
                .chars()
                .take_while(|&c| c.is_alphanumeric() || c == '_' || c == '$')
                .collect();
            if !param_name.is_empty() {
                params.push((param_name, i));
            }
        }

        if trimmed.contains("*/") && i > start {
            break;
        }
    }
    params
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

        if stripped.starts_with("function ")
            || stripped.contains("=> ")
            || stripped.contains("=>(")
            || stripped.contains('(')
        {
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

            let jsdoc_params = extract_jsdoc_param_names(&lines, block_start);
            if let Some(fn_line) = find_function_line(&lines, block_end) {
                let actual_params = extract_params(&lines, fn_line);
                for (jsdoc_name, tag_line) in &jsdoc_params {
                    if !actual_params.iter().any(|p| p == jsdoc_name) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: tag_line + 1,
                            column: 1,
                            rule_id: "jsdoc-check-param-names".into(),
                            message: format!(
                                "`@param {jsdoc_name}` does not match any function parameter. Actual params: [{}].",
                                actual_params.join(", ")
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
    fn flags_mismatched_param_name() {
        let source = r#"
/**
 * Greets a user.
 * @param nme - the name
 */
function greet(name: string) {}
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("nme"));
    }

    #[test]
    fn allows_matching_params() {
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
    fn flags_stale_param() {
        let source = r#"
/**
 * Process data.
 * @param input - data
 * @param options - config
 */
function process(input: string) {}
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("options"));
    }
}
