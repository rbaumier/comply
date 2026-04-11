//! no-useless-collection-argument — flag `new Set([])`, `new Map(undefined)`, etc.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const COLLECTIONS: &[&str] = &["Set", "Map", "WeakSet", "WeakMap"];

const USELESS_ARGS: &[&str] = &["[]", "undefined", "null", "\"\"", "''", "``"];

/// Check if a `new Collection(arg)` call has a useless argument.
fn find_useless_arg(line: &str) -> Option<&'static str> {
    for &col in COLLECTIONS {
        // Look for `new Set(`, `new Map(`, etc.
        let pattern = format!("new {col}(");
        if let Some(start) = line.find(&pattern) {
            let after_paren = start + pattern.len();
            let rest = &line[after_paren..];
            let rest_trimmed = rest.trim_start();
            for &arg in USELESS_ARGS {
                if let Some(after_arg) = rest_trimmed.strip_prefix(arg) {
                    // Verify the argument is followed by `)` (possibly with whitespace)
                    if after_arg.trim_start().starts_with(')') {
                        return Some(arg);
                    }
                }
            }
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(arg) = find_useless_arg(line) {
                let desc = match arg {
                    "[]" => "empty array",
                    "undefined" => "`undefined`",
                    "null" => "`null`",
                    "\"\"" | "''" | "``" => "empty string",
                    _ => "useless value",
                };
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-useless-collection-argument".into(),
                    message: format!("The {desc} argument is useless — remove it."),
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
    fn flags_new_set_empty_array() {
        let d = run("const s = new Set([]);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("empty array"));
    }

    #[test]
    fn flags_new_map_undefined() {
        let d = run("const m = new Map(undefined);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`undefined`"));
    }

    #[test]
    fn flags_new_weakset_null() {
        let d = run("const ws = new WeakSet(null);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`null`"));
    }

    #[test]
    fn flags_new_set_empty_string() {
        let d = run("const s = new Set(\"\");");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("empty string"));
    }

    #[test]
    fn allows_new_set_with_values() {
        assert!(run("const s = new Set([1, 2, 3]);").is_empty());
    }

    #[test]
    fn allows_new_set_no_args() {
        assert!(run("const s = new Set();").is_empty());
    }

    #[test]
    fn allows_new_map_with_entries() {
        assert!(run("const m = new Map([[\"a\", 1]]);").is_empty());
    }
}
