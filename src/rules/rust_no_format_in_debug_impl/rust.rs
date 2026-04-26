//! rust-no-format-in-debug-impl backend.
//!
//! For every `impl_item` whose trait is `Debug`/`fmt::Debug`/
//! `std::fmt::Debug`, find the `fn fmt(...)` method and scan its
//! body for `format!` macro invocations. Each one is a wasted
//! allocation that should be a `write!`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["impl_item"];

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
        let source_bytes = ctx.source.as_bytes();
        let Some(trait_node) = node.child_by_field_name("trait") else {
            return;
        };
        let Ok(trait_text) = trait_node.utf8_text(source_bytes) else {
            return;
        };
        if !is_debug_trait(trait_text) {
            return;
        }
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        collect_format_macros_in(body, source_bytes, ctx, diagnostics);
    }
}

fn is_debug_trait(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed == "Debug"
        || trimmed == "fmt::Debug"
        || trimmed == "std::fmt::Debug"
        || trimmed == "core::fmt::Debug"
}

fn collect_format_macros_in(
    body: tree_sitter::Node,
    source: &[u8],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut stack = vec![body];
    while let Some(node) = stack.pop() {
        if node.kind() == "macro_invocation"
            && let Some(macro_node) = node.child_by_field_name("macro")
            && let Ok(name) = macro_node.utf8_text(source)
            && name == "format"
        {
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-no-format-in-debug-impl".into(),
                message: "`format!` inside `Debug::fmt` allocates a \
                          throwaway `String`. Use `write!(f, \"...\", \
                          ...)` to stream directly into the formatter."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


    }

    #[test]
    fn flags_format_in_debug_impl() {
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(&format!("Foo({})", self.x))
            }
        }"#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_write_in_debug_impl() {
        let source = r#"impl Debug for Foo {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(f, "Foo({})", self.x)
            }
        }"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_format_in_other_impls() {
        let source = r#"impl Display for Foo {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(&format!("{}", self.x))
            }
        }"#;
        // Display is fair game — it's not on the same hot path as Debug.
        assert!(run_on(source).is_empty());
    }
}
