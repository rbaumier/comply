//! function-inside-loop backend — flag `function` declarations/expressions inside loops.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

fn is_word_boundary_before(bytes: &[u8], pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let prev = bytes[pos - 1];
    !prev.is_ascii_alphanumeric() && prev != b'_'
}

fn is_word_boundary_after(bytes: &[u8], end: usize) -> bool {
    if end >= bytes.len() {
        return true;
    }
    let next = bytes[end];
    !next.is_ascii_alphanumeric() && next != b'_'
}

fn keyword_at(src: &str, bytes: &[u8], i: usize, kw: &str) -> bool {
    let kw_len = kw.len();
    i + kw_len <= src.len()
        && &src[i..i + kw_len] == kw
        && is_word_boundary_before(bytes, i)
        && is_word_boundary_after(bytes, i + kw_len)
}

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, _tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let src = ctx.source;
        let bytes = src.as_bytes();
        let len = bytes.len();

        let mut loop_depth: usize = 0;
        let mut loop_brace_targets: Vec<usize> = Vec::new();
        let mut brace_depth: usize = 0;
        let mut i = 0;

        while i < len {
            let b = bytes[i];

            // Skip string literals.
            if b == b'"' || b == b'\'' || b == b'`' {
                let quote = b;
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == quote {
                        break;
                    }
                    i += 1;
                }
                i += 1;
                continue;
            }

            // Skip single-line comments.
            if b == b'/' && i + 1 < len && bytes[i + 1] == b'/' {
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }

            // Skip multi-line comments.
            if b == b'/' && i + 1 < len && bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < len {
                    if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                continue;
            }

            if b == b'{' {
                brace_depth += 1;
            } else if b == b'}' {
                brace_depth = brace_depth.saturating_sub(1);
                while let Some(&target) = loop_brace_targets.last() {
                    if brace_depth <= target {
                        loop_brace_targets.pop();
                        loop_depth -= 1;
                    } else {
                        break;
                    }
                }
            }

            if (keyword_at(src, bytes, i, "for") && {
                let rest = src[i + 3..].trim_start();
                rest.starts_with('(')
            }) || (keyword_at(src, bytes, i, "while") && {
                let rest = src[i + 5..].trim_start();
                rest.starts_with('(')
            }) || keyword_at(src, bytes, i, "do")
            {
                loop_depth += 1;
                loop_brace_targets.push(brace_depth);
                if keyword_at(src, bytes, i, "while") {
                    i += 5;
                } else if keyword_at(src, bytes, i, "for") {
                    i += 3;
                } else {
                    i += 2;
                }
                continue;
            }

            if loop_depth > 0 && keyword_at(src, bytes, i, "function") {
                let after = i + 8;
                if after < len && (bytes[after] == b' ' || bytes[after] == b'(') {
                    let line = src[..i].matches('\n').count() + 1;
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line,
                        column: 1,
                        rule_id: "function-inside-loop".into(),
                        message: "Function declared inside a loop — move it outside \
                                  or use an arrow function."
                            .into(),
                        severity: Severity::Warning,
                    });
                }
                i += 8;
                continue;
            }

            i += 1;
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_function_in_for_loop() {
        let src = "for (let i = 0; i < 10; i++) {\n    function inner() { return i; }\n}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_function_outside_loop() {
        let src = "function outer() { return 1; }\nfor (let i = 0; i < 10; i++) {\n    outer();\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_arrow_function_in_loop() {
        let src = "for (let i = 0; i < 10; i++) {\n    const fn = (x) => x + i;\n}";
        assert!(run_on(src).is_empty());
    }
}
