use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Pattern 1: empty blocks — `catch {}`, `else {}`, `default:`, `{ }` etc.
            // Detect lines where `{` and `}` appear with only whitespace between.
            for keyword in &["catch", "else", "default"] {
                if let Some(pos) = trimmed.find(keyword) {
                    let after = trimmed[pos + keyword.len()..].trim();
                    // Handle single-line empty block: `catch {}` or `catch { }`
                    let after_paren = if after.starts_with('(') {
                        // skip catch(e) part
                        after.find(')').map(|p| after[p + 1..].trim()).unwrap_or(after)
                    } else if let Some(rest) = after.strip_prefix(':') {
                        // skip `default:` colon
                        rest.trim()
                    } else {
                        after
                    };
                    if (after_paren == "{}" || after_paren == "{ }") && !has_nearby_comment(&lines, idx, trimmed) {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: 1,
                            rule_id: "justify-inaction".into(),
                            message: format!("Empty `{keyword}` block without an explaining comment — add a comment on the preceding line."),
                            severity: Severity::Warning,
                        });
                    }
                }
            }

            // Also detect multi-line empty blocks:
            //   catch(e) {
            //   }
            if idx + 1 < lines.len() {
                let next_trimmed = lines[idx + 1].trim();
                if next_trimmed == "}" {
                    for keyword in &["catch", "else", "default"] {
                        if trimmed.contains(keyword) && trimmed.ends_with('{') && !has_nearby_comment(&lines, idx, trimmed) {
                            // Make sure there's nothing between { and }
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: idx + 1,
                                column: 1,
                                rule_id: "justify-inaction".into(),
                                message: format!("Empty `{keyword}` block without an explaining comment — add a comment on the preceding line."),
                                severity: Severity::Warning,
                            });
                        }
                    }
                }
            }

            // Pattern 2: bare `return;` without preceding comment.
            if trimmed == "return;" && !has_nearby_comment(&lines, idx, trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "justify-inaction".into(),
                    message: "Early `return;` without an explaining comment — add a comment on the preceding line.".into(),
                    severity: Severity::Warning,
                });
            }
        }
        diagnostics
    }
}

fn has_nearby_comment(lines: &[&str], idx: usize, line: &str) -> bool {
    // Inline comment on the same line.
    if line.contains("//") || line.contains("/*") {
        return true;
    }
    // Comment on the preceding line.
    if idx > 0 {
        let prev = lines[idx - 1].trim();
        if prev.contains("//") || prev.ends_with("*/") || prev.starts_with("/*") || prev.starts_with('*') {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_empty_catch() {
        let src = "try { x(); } catch(e) {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_catch_with_comment() {
        let src = "// Intentionally swallowed — retried in the outer loop.\ntry { x(); } catch(e) {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_bare_return() {
        let src = "function foo() {\n  if (!x)\n  return;\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_return_with_comment() {
        let src = "function foo() {\n  // Guard: x is validated upstream.\n  return;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_empty_else() {
        let src = "if (x) { doA(); } else {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_catch_with_inline_comment() {
        let src = "try { x(); } catch(e) {} // swallowed intentionally";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_multiline_empty_catch() {
        let src = "try { x(); } catch(e) {\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_multiline_catch_with_comment_above() {
        let src = "// We retry elsewhere.\ntry { x(); } catch(e) {\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_empty_default_block() {
        let src = "    default: {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_block_comment_above() {
        let src = "/* intentional no-op */\ntry { x(); } catch(e) {}";
        assert!(run(src).is_empty());
    }
}
