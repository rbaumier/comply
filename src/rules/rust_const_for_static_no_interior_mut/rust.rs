//! rust-const-for-static-no-interior-mut backend.
//!
//! For each `static_item`:
//! - skip if `mut` (already covered by `rust-no-static-mut`);
//! - skip if the type or value mentions a known-interior-mutability type
//!   (`Cell`, `RefCell`, `UnsafeCell`, `Mutex`, `RwLock`, `OnceLock`,
//!   `OnceCell`, `LazyLock`, `Lazy`, `AtomicXxx`);
//! - skip if the value is not a literal-flavoured expression. We
//!   conservatively allow `integer_literal`, `float_literal`,
//!   `string_literal`, `char_literal`, `boolean_literal`, plus `&"…"` and
//!   `b"…"`. Anything else (function calls, `vec![]`, etc.) is left alone
//!   — those usually can't be `const` anyway and the false-positive cost
//!   is high.
//! - skip if the static's address is taken (`&NAME`) in scope — a stable,
//!   unique address is being relied upon (e.g. an FFI pointer handed to C),
//!   which a `const` inlined at each use site would not provide.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["static_item"];

const INTERIOR_MUT_MARKERS: &[&str] = &[
    "Cell",
    "RefCell",
    "UnsafeCell",
    "Mutex",
    "RwLock",
    "OnceLock",
    "OnceCell",
    "LazyLock",
    "Lazy",
    "Atomic",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        // Skip `static mut`.
        let mut cursor = node.walk();
        if node
            .children(&mut cursor)
            .any(|c| c.kind() == "mutable_specifier")
        {
            return;
        }
        let Some(ty) = node.child_by_field_name("type") else {
            return;
        };
        let Ok(ty_text) = ty.utf8_text(source) else {
            return;
        };
        if INTERIOR_MUT_MARKERS.iter().any(|m| ty_text.contains(m)) {
            return;
        }
        let Some(value) = node.child_by_field_name("value") else {
            return;
        };
        if !is_literal_value(value) {
            return;
        }
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .unwrap_or("FOO");
        // Skip when the address is taken (`&NAME`): const-inlining would remove
        // the stable, unique address the code relies on. Scope is the enclosing
        // function for a function-local static, else the whole file.
        if address_taken(address_scope(node), name, source) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "rust-const-for-static-no-interior-mut",
            format!(
                "`static {name}` has a literal value and no interior \
                 mutability — use `const {name}` so the value inlines \
                 at every use site instead of reserving a fixed address."
            ),
            Severity::Warning,
        ));
    }
}

fn is_literal_value(node: tree_sitter::Node) -> bool {
    match node.kind() {
        "integer_literal" | "float_literal" | "string_literal" | "raw_string_literal"
        | "char_literal" | "boolean_literal" | "negative_literal" => true,
        "reference_expression" => node
            .child_by_field_name("value")
            .map(is_literal_value)
            .unwrap_or(false),
        _ => false,
    }
}

/// The scope to scan for address-of on a static: the nearest enclosing
/// `function_item` for a function-local static, else the top-level ancestor
/// (`source_file`) for a module-level one.
fn address_scope(node: tree_sitter::Node) -> tree_sitter::Node {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "function_item" {
            return parent;
        }
        current = parent;
    }
    current
}

/// True when `scope`'s subtree contains a `reference_expression` (`&NAME`) whose
/// operand is the bare identifier `name` — the static's address being taken.
fn address_taken(scope: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    if scope.kind() == "reference_expression"
        && scope.child_by_field_name("value").is_some_and(|value| {
            value.kind() == "identifier" && value.utf8_text(source).is_ok_and(|text| text == name)
        })
    {
        return true;
    }
    let mut cursor = scope.walk();
    scope
        .children(&mut cursor)
        .any(|child| address_taken(child, name, source))
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_static_int_literal() {
        let src = "static MAX: u32 = 100;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_static_str_literal() {
        let src = r#"static NAME: &str = "comply";"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_static_bool_literal() {
        let src = "static ENABLED: bool = true;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_static_atomic() {
        let src = "static COUNTER: AtomicU32 = AtomicU32::new(0);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_static_oncelock() {
        let src = "static CFG: OnceLock<String> = OnceLock::new();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_static_mut() {
        let src = "static mut COUNTER: u32 = 0;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_static_with_function_call_value() {
        let src = "static V: Vec<u32> = compute();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_function_local_static_whose_address_is_taken() {
        let src = r#"
            fn f(nva: &mut Vec<Nv>) {
                if cond {
                    static ZERO: u8 = 0;
                    nva.push(Nv {
                        name: &ZERO as *const _ as *mut _,
                        value: &ZERO as *const _ as *mut _,
                    });
                }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_module_level_static_whose_address_is_taken() {
        let src = "static X: u32 = 5; fn g() -> *const u32 { &X as *const u32 }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_module_level_static_used_by_value_only() {
        let src = "static Y: u32 = 5; fn h() -> u32 { Y * 2 }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_function_local_static_used_by_value_only() {
        let src = "fn k() { static Z: u32 = 7; let _ = Z + 1; }";
        assert_eq!(run_on(src).len(), 1);
    }
}
