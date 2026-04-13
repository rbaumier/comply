//! no-useless-collection-argument AST backend — flag `new Set([])`, `new Map(undefined)`, etc.

use crate::diagnostic::{Diagnostic, Severity};

const COLLECTIONS: &[&str] = &["Set", "Map", "WeakSet", "WeakMap"];

const USELESS_ARGS: &[&str] = &["[]", "undefined", "null", "\"\"", "''", "``"];

/// Check if a `new Collection(arg)` call has a useless argument.
fn find_useless_arg(line: &str) -> Option<&'static str> {
    for &col in COLLECTIONS {
        let pattern = format!("new {col}(");
        if let Some(start) = line.find(&pattern) {
            let after_paren = start + pattern.len();
            let rest = &line[after_paren..];
            let rest_trimmed = rest.trim_start();
            for &arg in USELESS_ARGS {
                if let Some(after_arg) = rest_trimmed.strip_prefix(arg)
                    && after_arg.trim_start().starts_with(')') {
                        return Some(arg);
                    }
            }
        }
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in text.lines().enumerate() {
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
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_new_set_empty_array() {
        let d = run_on("const s = new Set([]);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("empty array"));
    }

    #[test]
    fn flags_new_map_undefined() {
        let d = run_on("const m = new Map(undefined);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`undefined`"));
    }

    #[test]
    fn flags_new_weakset_null() {
        let d = run_on("const ws = new WeakSet(null);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`null`"));
    }

    #[test]
    fn flags_new_set_empty_string() {
        let d = run_on("const s = new Set(\"\");");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("empty string"));
    }

    #[test]
    fn allows_new_set_with_values() {
        assert!(run_on("const s = new Set([1, 2, 3]);").is_empty());
    }

    #[test]
    fn allows_new_set_no_args() {
        assert!(run_on("const s = new Set();").is_empty());
    }

    #[test]
    fn allows_new_map_with_entries() {
        assert!(run_on("const m = new Map([[\"a\", 1]]);").is_empty());
    }
}
