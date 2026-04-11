use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

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

fn has_return_value(lines: &[&str], fn_line: usize) -> bool {
    let mut combined = String::new();
    let mut brace_depth = 0i32;
    let mut started = false;

    for line in &lines[fn_line..] {
        combined.push_str(line);
        combined.push('\n');
        for ch in line.chars() {
            if ch == '{' {
                brace_depth += 1;
                started = true;
            } else if ch == '}' {
                brace_depth -= 1;
                if started && brace_depth == 0 {
                    return has_return_expr(&combined);
                }
            }
        }
    }
    if !started && combined.contains("=>") {
        return true;
    }
    has_return_expr(&combined)
}

fn has_return_expr(body: &str) -> bool {
    let mut search_from = 0;
    while let Some(pos) = body[search_from..].find("return") {
        let abs = search_from + pos;
        if abs > 0 {
            let prev = body.as_bytes()[abs - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'$' {
                search_from = abs + 6;
                continue;
            }
        }
        let after_return = &body[abs + 6..];
        match after_return.chars().next() {
            Some(c) if c.is_ascii_alphanumeric() || c == '_' || c == '$' => {
                search_from = abs + 6;
                continue;
            }
            _ => {}
        }
        let rest = after_return.trim_start();
        if rest.starts_with(';') || rest.starts_with('}') || rest.is_empty() {
            search_from = abs + 6;
            continue;
        }
        return true;
    }
    false
}

fn find_returns_tag_line(lines: &[&str], start: usize, end: usize) -> Option<usize> {
    for (idx, line) in lines.iter().enumerate().skip(start).take(end - start + 1) {
        let content = line.trim().trim_start_matches('*').trim();
        if content.starts_with("@returns") || content.starts_with("@return ") || content == "@return"
        {
            return Some(idx);
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

            if let Some(returns_line) = find_returns_tag_line(&lines, block_start, block_end)
                && let Some(fn_line) = find_function_line(&lines, block_end)
                && !has_return_value(&lines, fn_line)
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: returns_line + 1,
                    column: 1,
                    rule_id: "jsdoc-returns-check".into(),
                    message: "`@returns` is documented but the function never returns a value. Remove the stale tag.".into(),
                    severity: Severity::Warning,
                });
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
    fn flags_returns_on_void_function() {
        let source = r#"
/**
 * Logs a message.
 * @param msg - the message
 * @returns the result
 */
function log(msg: string) { console.log(msg); }
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("never returns"));
    }

    #[test]
    fn allows_returns_on_returning_function() {
        let source = r#"
/**
 * Doubles a number.
 * @param x - input
 * @returns the doubled value
 */
function double(x: number) { return x * 2; }
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_no_returns_tag() {
        let source = r#"
/**
 * Logs a message.
 * @param msg - the message
 */
function log(msg: string) { console.log(msg); }
"#;
        assert!(run(source).is_empty());
    }
}
