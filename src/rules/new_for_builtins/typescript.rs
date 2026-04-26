//! new-for-builtins backend — enforce `new` for builtins, disallow for Symbol/BigInt.

use crate::diagnostic::{Diagnostic, Severity};

/// Builtins that MUST be called with `new`.
const ENFORCE_NEW: &[&str] = &[
    "Object",
    "Array",
    "ArrayBuffer",
    "DataView",
    "Date",
    "Error",
    "Function",
    "Map",
    "WeakMap",
    "Set",
    "WeakSet",
    "Promise",
    "RegExp",
    "SharedArrayBuffer",
    "Proxy",
    "WeakRef",
    "FinalizationRegistry",
];

/// Builtins that MUST NOT be called with `new`.
const DISALLOW_NEW: &[&str] = &["Symbol", "BigInt"];

crate::ast_check! { on ["call_expression", "new_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        // `Map()` without `new` — should be `new Map()`.
        "call_expression" => {
            let Some(func) = node.child_by_field_name("function") else { return };
            if func.kind() != "identifier" { return; }

            let name = func.utf8_text(source).unwrap_or("");
            if !ENFORCE_NEW.contains(&name) { return; }

            // Make sure the parent is NOT a `new_expression` — tree-sitter
            // parses `new Map()` as new_expression > identifier + arguments,
            // not as call_expression. So if we see `Map(...)` as a call_expression,
            // it genuinely lacks `new`.
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "new-for-builtins".into(),
                message: format!("Use `new {name}()` instead of `{name}()`."),
                severity: Severity::Error,
                span: None,
            });
        }
        // `new Symbol()` — should be `Symbol()`.
        "new_expression" => {
            let Some(ctor) = node.child_by_field_name("constructor") else { return };
            if ctor.kind() != "identifier" { return; }

            let name = ctor.utf8_text(source).unwrap_or("");
            if !DISALLOW_NEW.contains(&name) { return; }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "new-for-builtins".into(),
                message: format!("Use `{name}()` instead of `new {name}()`. `{name}` is not a constructor."),
                severity: Severity::Error,
                span: None,
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_map_without_new() {
        let d = run_on("const m = Map();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("new Map()"));
    }

    #[test]
    fn flags_set_without_new() {
        let d = run_on("const s = Set();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("new Set()"));
    }

    #[test]
    fn flags_promise_without_new() {
        let d = run_on("const p = Promise(() => {});");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("new Promise()"));
    }

    #[test]
    fn flags_new_symbol() {
        let d = run_on("const s = new Symbol('foo');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Symbol()"));
        assert!(d[0].message.contains("not a constructor"));
    }

    #[test]
    fn flags_new_bigint() {
        let d = run_on("const b = new BigInt(42);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("BigInt()"));
    }

    #[test]
    fn allows_new_map() {
        assert!(run_on("const m = new Map();").is_empty());
    }

    #[test]
    fn allows_new_set() {
        assert!(run_on("const s = new Set();").is_empty());
    }

    #[test]
    fn allows_symbol_factory() {
        assert!(run_on("const s = Symbol('foo');").is_empty());
    }

    #[test]
    fn allows_custom_class_without_new() {
        assert!(run_on("const x = myFunction();").is_empty());
    }

    #[test]
    fn allows_new_custom_class() {
        assert!(run_on("const x = new MyClass();").is_empty());
    }
}
