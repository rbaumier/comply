use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn tag_order(tag: &str) -> Option<u8> {
    match tag {
        "param" => Some(0),
        "returns" | "return" => Some(1),
        "throws" | "exception" => Some(2),
        "example" => Some(3),
        _ => None,
    }
}

fn extract_tag_name(content: &str) -> Option<&str> {
    let rest = content.strip_prefix('@')?;
    let tag = rest
        .split(|c: char| c.is_whitespace() || c == '{')
        .next()?;
    if tag.is_empty() {
        return None;
    }
    Some(tag)
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

            if trimmed.contains("*/") && trimmed != "/**" {
                i += 1;
                continue;
            }

            i += 1;
            while i < lines.len() {
                if lines[i].trim().contains("*/") {
                    block_end = i;
                    break;
                }
                i += 1;
            }

            // Collect ordered tags with their positions.
            let mut seen_tags: Vec<(u8, &str, usize)> = Vec::new();
            for (line_idx, line) in lines.iter().enumerate().skip(block_start).take(block_end - block_start + 1) {
                let content = line.trim().trim_start_matches('*').trim();
                if let Some(tag) = extract_tag_name(content)
                    && let Some(order) = tag_order(tag)
                {
                    seen_tags.push((order, tag, line_idx));
                }
            }

            for window in seen_tags.windows(2) {
                let (prev_order, prev_tag, _) = window[0];
                let (cur_order, cur_tag, cur_line) = window[1];
                if cur_order < prev_order {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: cur_line + 1,
                        column: 1,
                        rule_id: "jsdoc-sort-tags".into(),
                        message: format!(
                            "`@{cur_tag}` must come before `@{prev_tag}`. Canonical order: @param, @returns, @throws, @example."
                        ),
                        severity: Severity::Warning,
                    });
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
    fn flags_returns_before_param() {
        let source = r#"
/**
 * Does something.
 * @returns the result
 * @param x - input
 */
function foo(x: number) { return x; }
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("@param"));
        assert!(d[0].message.contains("before"));
    }

    #[test]
    fn allows_correct_order() {
        let source = r#"
/**
 * Does something.
 * @param x - input
 * @returns the result
 * @throws if invalid
 * @example foo(1)
 */
function foo(x: number) { return x; }
"#;
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_example_before_throws() {
        let source = r#"
/**
 * Does something.
 * @param x - input
 * @example foo(1)
 * @throws Error
 */
function foo(x: number) { return x; }
"#;
        let d = run(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("@throws"));
    }
}
