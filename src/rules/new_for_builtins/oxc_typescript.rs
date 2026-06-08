//! new-for-builtins OXC backend — enforce `new` for builtins, disallow for Symbol/BigInt.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

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

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::NewExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            // `Map()` without `new` — should be `new Map()`.
            AstKind::CallExpression(call) => {
                let Expression::Identifier(ident) = &call.callee else {
                    return;
                };
                let name = ident.name.as_str();
                if !ENFORCE_NEW.contains(&name) {
                    return;
                }
                if is_name_locally_bound(semantic, ident) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Use `new {name}()` instead of `{name}()`."),
                    severity: Severity::Error,
                    span: None,
                });
            }
            // `new Symbol()` — should be `Symbol()`.
            AstKind::NewExpression(new_expr) => {
                let Expression::Identifier(ident) = &new_expr.callee else {
                    return;
                };
                let name = ident.name.as_str();
                if !DISALLOW_NEW.contains(&name) {
                    return;
                }
                if is_name_locally_bound(semantic, ident) {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Use `{name}()` instead of `new {name}()`. `{name}` is not a constructor."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

/// Check whether the identifier has a local binding (parameter, variable, or import).
fn is_name_locally_bound(
    semantic: &oxc_semantic::Semantic,
    ident: &oxc_ast::ast::IdentifierReference,
) -> bool {
    let scoping = semantic.scoping();
    let name = ident.name.as_str();
    // Check if any symbol with this name exists in any scope.
    for sym_id in scoping.symbol_ids() {
        if scoping.symbol_name(sym_id) == name {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
