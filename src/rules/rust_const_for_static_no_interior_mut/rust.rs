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
//! - skip if the static carries a symbol-export attribute (`#[no_mangle]`,
//!   `#[export_name = "…"]`, `#[link_section = "…"]`, including the edition-2024
//!   `#[unsafe(…)]` wrapper): each pins a real, uniquely-addressed linker/FFI
//!   symbol, which a `const` (inlined, with no address and no symbol) cannot
//!   carry, so the transformation is invalid.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::has_symbol_export_attribute;

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
        // A static carrying a symbol-export attribute defines a real,
        // uniquely-addressed linker/FFI symbol (jemalloc `malloc_conf`, a CRT
        // init pointer, a `#[no_mangle]` C entry point). A `const` is inlined at
        // each use site with no address and no symbol, so the compiler rejects
        // these attributes on it — the suggested rewrite is invalid.
        if has_symbol_export_attribute(node, source) {
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

    #[test]
    fn allows_static_with_unsafe_export_name_attr() {
        // openobserve jemalloc profiling: the static exports the `malloc_conf`
        // linker symbol the jemalloc C runtime reads at startup (issue #7754).
        let src = r#"
            #[allow(non_upper_case_globals)]
            #[unsafe(export_name = "malloc_conf")]
            pub static malloc_conf: &[u8] = b"prof:true,prof_active:true\0";
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_static_with_no_mangle_attr() {
        let src = "#[no_mangle] static X: u8 = 1;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_static_with_export_name_attr() {
        let src = r#"#[export_name = "y"] static Y: u8 = 1;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_static_with_link_section_attr() {
        let src = r#"#[link_section = ".foo"] static Z: u8 = 1;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_static_with_unsafe_no_mangle_attr() {
        let src = "#[unsafe(no_mangle)] static A: u8 = 1;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_static_with_unsafe_link_section_attr() {
        let src = r#"#[unsafe(link_section = ".x")] static B: u8 = 1;"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_static_with_no_export_attr() {
        let src = "static PLAIN: u8 = 1;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_static_with_unrelated_attr() {
        // An unrelated attribute must not neuter the rule: `const` is a valid
        // rewrite here, so the static is still flagged.
        let src = "#[allow(dead_code)] static Q: u8 = 1;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_static_with_export_word_only_as_attr_argument() {
        // Match on the attribute PATH, not its arguments: `no_mangle` here is an
        // argument to `#[allow(…)]`, not the attribute path, so `const` is still
        // a valid rewrite and the static stays flagged.
        let src = "#[allow(no_mangle)] static X: u8 = 1;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_static_with_path_qualified_no_mangle_attr() {
        // A `::`-qualified attribute path matches on its last segment.
        let src = "#[core::no_mangle] static X: u8 = 1;";
        assert!(run_on(src).is_empty());
    }
}
