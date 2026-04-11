use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects `.apply(` calls that should use `Reflect.apply()`.
///
/// Patterns flagged:
/// - `fn.apply(null, args)` / `fn.apply(this, args)`
/// - `Function.prototype.apply.call(fn, thisArg, args)`
fn find_apply_violation(line: &str) -> Option<&'static str> {
    let stripped = strip_strings_and_comments(line);

    // `Function.prototype.apply.call(` — the verbose pattern
    if stripped.contains("Function.prototype.apply.call(") {
        return Some(
            "Prefer `Reflect.apply(fn, thisArg, args)` over `Function.prototype.apply.call(fn, thisArg, args)`.",
        );
    }

    // `.apply(` — direct apply call
    // We look for `.apply(` but skip `Reflect.apply(` which is already correct.
    if stripped.contains(".apply(") && !stripped.contains("Reflect.apply(") {
        // Find each `.apply(` occurrence
        let mut search = stripped.as_str();
        while let Some(pos) = search.find(".apply(") {
            // Check it's not preceded by `Reflect`
            let before = &search[..pos];
            if !before.ends_with("Reflect") {
                return Some(
                    "Prefer `Reflect.apply(fn, thisArg, args)` over `fn.apply(thisArg, args)`.",
                );
            }
            search = &search[pos + 7..];
        }
    }

    None
}

/// Strip string literals and single-line comments.
fn strip_strings_and_comments(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '/' if chars.peek() == Some(&'/') => break,
            '\'' | '"' | '`' => {
                let quote = c;
                let mut escape_next = false;
                for inner in chars.by_ref() {
                    if escape_next {
                        escape_next = false;
                        continue;
                    }
                    if inner == '\\' {
                        escape_next = true;
                    } else if inner == quote {
                        break;
                    }
                }
                result.push(' ');
            }
            _ => result.push(c),
        }
    }
    result
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(msg) = find_apply_violation(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-reflect-apply".into(),
                    message: msg.into(),
                    severity: Severity::Warning,
                });
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
    fn flags_direct_apply() {
        let d = run("fn.apply(null, args);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-reflect-apply");
    }

    #[test]
    fn flags_apply_with_this() {
        let d = run("foo.bar.apply(this, args);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_function_prototype_apply_call() {
        let d = run("Function.prototype.apply.call(fn, null, args);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_reflect_apply() {
        assert!(run("Reflect.apply(fn, null, args);").is_empty());
    }

    #[test]
    fn allows_non_apply_method() {
        assert!(run("fn.call(null, args);").is_empty());
    }

    #[test]
    fn ignores_string_literal() {
        assert!(run(r#"const s = "fn.apply(null, args)";"#).is_empty());
    }

    #[test]
    fn ignores_comment() {
        assert!(run("// fn.apply(null, args)").is_empty());
    }
}
