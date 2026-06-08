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
}
