//! no-nested-functions Rust backend — flag free `fn`s nested 3+ levels deep.
//!
//! Only free helper functions declared inside another function body count as
//! nesting. An associated method (a `function_item` whose parent is the
//! `declaration_list` of an `impl`/`trait`) belongs to its type's interface, so
//! it is neither flagged nor counted toward depth — this is the idiomatic
//! local-struct + `impl` block pattern (e.g. serde visitors).

use crate::diagnostic::{Diagnostic, Severity};

/// A `function_item` is an associated method when it sits directly inside the
/// `declaration_list` body of an `impl` or `trait` block.
fn is_assoc_method(node: tree_sitter::Node) -> bool {
    node.parent().map(|p| p.kind()) == Some("declaration_list")
}

crate::ast_check! { on ["function_item"] => |node, _source, ctx, diagnostics|
    // Methods of local structs/traits are interface members, not nested helpers.
    if is_assoc_method(node) {
        return;
    }
    // Walk ancestors to count enclosing free-function bodies.
    let mut depth = 0usize;
    let mut parent = node.parent();
    while let Some(p) = parent {
        if p.kind() == "function_item" && !is_assoc_method(p) {
            depth += 1;
        }
        parent = p.parent();
    }
    if depth >= 2 {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-nested-functions".into(),
            message: format!(
                "Function declared at nesting depth {} \u{2014} extract to module scope.",
                depth
            ),
            severity: Severity::Warning,
            span: None,
        });
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
    fn flags_deeply_nested_function() {
        let src = r#"fn outer() {
    fn middle() {
        fn too_deep() {
            return;
        }
    }
}"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-nested-functions");
        assert!(d[0].message.contains("depth 2"));
    }

    #[test]
    fn allows_two_levels() {
        let src = r#"fn outer() {
    fn inner() {
        return;
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_top_level_function() {
        let src = "fn foo() { return; }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_impl_methods_of_local_struct_in_fn() {
        // serde visitor pattern: a local struct + `impl` block nested two
        // levels deep inside `fn` bodies. The methods belong to the struct's
        // interface, not a nested free helper, so they must not be flagged.
        let src = r#"fn deserialize_outer() {
    fn deserialize_field() {
        struct FieldVisitor;
        impl FieldVisitor {
            fn visit_str(&self, value: &str) {
                let _ = value;
            }
            fn visit_bytes(&self, value: &[u8]) {
                let _ = value;
            }
        }
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_free_helper_nested_in_two_free_fns_inside_impl_method() {
        // A genuine free helper `fn` enclosed by two free-function bodies is
        // still flagged, even when the outermost free fn lives inside an impl
        // method (the impl method itself does not count toward depth).
        let src = r#"impl Foo {
    fn method(&self) {
        fn outer_helper() {
            fn middle_helper() {
                fn inner_helper() {
                    return;
                }
            }
        }
    }
}"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("depth 2"));
    }
}
