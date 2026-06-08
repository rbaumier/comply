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

/// Check whether `name` is declared as a local binding (parameter, variable,
/// or import) in any scope between `node` and the program root.
fn is_name_locally_bound(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut cursor = node.parent();
    while let Some(ancestor) = cursor {
        let kind = ancestor.kind();

        // Function-like: check formal_parameters for a matching param name.
        if matches!(
            kind,
            "function_declaration"
                | "function_expression"
                | "arrow_function"
                | "method_definition"
                | "generator_function"
                | "generator_function_declaration"
        ) {
            if let Some(params) = ancestor.child_by_field_name("parameters") {
                if params_contain_name(params, source, name) {
                    return true;
                }
            }
        }

        // Scan direct children for variable_declarator or import bindings.
        let child_count = ancestor.child_count();
        for i in 0..child_count {
            let child = ancestor.child(i).unwrap();
            // Only consider declarations that appear *before* our node (or
            // at the same level for imports/hoisted declarations).
            if declarator_binds_name(child, source, name) {
                return true;
            }
        }

        // Stop at the program root — anything there is global scope.
        if kind == "program" {
            break;
        }

        cursor = ancestor.parent();
    }
    false
}

/// True if a `formal_parameters` node contains a parameter binding `name`.
fn params_contain_name(params: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let count = params.child_count();
    for i in 0..count {
        let param = params.child(i).unwrap();
        let pk = param.kind();
        if pk == "required_parameter" || pk == "optional_parameter" {
            if let Some(pat) = param.child_by_field_name("pattern") {
                if pat.kind() == "identifier" && pat.utf8_text(source).unwrap_or("") == name {
                    return true;
                }
            }
        }
        // Plain identifier param (no type annotation).
        if pk == "identifier" && param.utf8_text(source).unwrap_or("") == name {
            return true;
        }
    }
    false
}

/// True if `child` is a variable declarator or import binding that introduces `name`.
fn declarator_binds_name(child: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let ck = child.kind();

    // `const Map = ...` / `let Map = ...`
    if ck == "lexical_declaration" || ck == "variable_declaration" {
        let count = child.child_count();
        for i in 0..count {
            let decl = child.child(i).unwrap();
            if decl.kind() == "variable_declarator" {
                if let Some(n) = decl.child_by_field_name("name") {
                    if n.kind() == "identifier" && n.utf8_text(source).unwrap_or("") == name {
                        return true;
                    }
                }
            }
        }
    }

    // `import Map from '...'` or `import { Map } from '...'`
    if ck == "import_statement" {
        let count = child.child_count();
        for i in 0..count {
            let part = child.child(i).unwrap();
            // Default import: import_clause > identifier
            if part.kind() == "import_clause" {
                let cc = part.child_count();
                for j in 0..cc {
                    let inner = part.child(j).unwrap();
                    if inner.kind() == "identifier" && inner.utf8_text(source).unwrap_or("") == name
                    {
                        return true;
                    }
                    // Named imports: import_clause > named_imports > import_specifier
                    if inner.kind() == "named_imports" {
                        let nc = inner.child_count();
                        for k in 0..nc {
                            let spec = inner.child(k).unwrap();
                            if spec.kind() == "import_specifier" {
                                // alias takes priority: `import { X as Map }`
                                let binding = spec
                                    .child_by_field_name("alias")
                                    .or_else(|| spec.child_by_field_name("name"));
                                if let Some(b) = binding {
                                    if b.utf8_text(source).unwrap_or("") == name {
                                        return true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    false
}

crate::ast_check! { on ["call_expression", "new_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        // `Map()` without `new` — should be `new Map()`.
        "call_expression" => {
            let Some(func) = node.child_by_field_name("function") else { return };
            if func.kind() != "identifier" { return; }

            let name = func.utf8_text(source).unwrap_or("");
            if !ENFORCE_NEW.contains(&name) { return; }

            if is_name_locally_bound(node, source, name) { return; }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
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

            if is_name_locally_bound(node, source, name) { return; }

            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    // --- Shadowing regression tests (ISS-029) ---

    #[test]
    fn allows_shadowed_by_function_param() {
        let src = r#"
function make(Map: () => unknown) {
  return Map();
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_shadowed_by_const() {
        let src = r#"
const Map = () => ({});
const m = Map();
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_shadowed_by_let() {
        let src = r#"
let Promise = (cb: any) => cb();
const p = Promise(() => {});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_shadowed_by_import() {
        let src = r#"
import { Map } from './custom-map';
const m = Map();
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_shadowed_by_import_alias() {
        let src = r#"
import { MyMap as Map } from './custom-map';
const m = Map();
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_shadowed_by_default_import() {
        let src = r#"
import Map from './custom-map';
const m = Map();
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_disallow_new_shadowed_by_param() {
        // Symbol is a local param here, not the global.
        let src = r#"
function make(Symbol: new (s: string) => unknown) {
  return new Symbol('foo');
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_global_map_call() {
        // No local binding — should still flag.
        let d = run_on("const m = Map();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("new Map()"));
    }

    #[test]
    fn still_flags_global_promise_call() {
        let d = run_on("const p = Promise(() => {});");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("new Promise()"));
    }
}
