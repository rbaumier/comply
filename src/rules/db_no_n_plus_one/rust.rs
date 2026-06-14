//! db-no-n-plus-one Rust backend.
//!
//! Flag `.await` on DB-like calls inside loops. In Rust this looks like
//! `query(...).await` inside `for`/`while`/`loop` blocks.
//!
//! Inline `#[cfg(test)]` modules are exempt: parametrized tests routinely
//! create a fresh in-memory datastore per loop iteration and run one query
//! against it, which is not the N+1 antipattern (each iteration has isolated
//! storage and cannot be batched). Path-based test files are handled by
//! `skip_in_test_dir`; this covers tests embedded in production `src/` files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_test_context;

const QUERY_METHODS: &[&str] = &[
    "query",
    "execute",
    "fetch_one",
    "fetch_all",
    "fetch_optional",
    "find",
    "insert",
    "update",
    "delete",
];

fn is_db_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    let text = node.utf8_text(source).unwrap_or("");
    QUERY_METHODS
        .iter()
        .any(|m| text.contains(&format!(".{m}(")))
}

fn is_inside_loop(node: tree_sitter::Node) -> bool {
    let mut parent = node.parent();
    while let Some(p) = parent {
        match p.kind() {
            "for_expression" | "while_expression" | "loop_expression" => return true,
            "function_item" | "closure_expression" => return false,
            _ => {}
        }
        parent = p.parent();
    }
    false
}

crate::ast_check! { on ["await_expression"] => |node, source, ctx, diagnostics|
    if !is_inside_loop(node) {
        return;
    }

    if is_in_test_context(node, source) {
        return;
    }

    let Some(inner) = node.named_child(0) else { return };
    if !is_db_call(inner, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "db-no-n-plus-one".into(),
        message: "Awaited DB query inside a loop — use a batch query or JOIN.".into(),
        severity: Severity::Error,
        span: None,
    });
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
    fn flags_query_in_loop() {
        let src = "async fn f(ids: Vec<i32>) { for id in ids { db.query(id).await; } }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_query_outside_loop() {
        let src = "async fn f() { db.query(1).await; }";
        assert!(run_on(src).is_empty());
    }

    // Issue #1470: parametrized tests create a fresh in-memory datastore per
    // loop iteration and run one query against it — not an N+1 query. An inline
    // `#[cfg(test)]` module in a production `src/` file must be exempt.
    #[test]
    fn allows_query_in_loop_inside_cfg_test_module() {
        let src = r#"
            #[cfg(test)]
            mod tests {
                async fn t() {
                    for level in &test_levels {
                        for case in &test_cases {
                            let ds = Datastore::new("memory").await.unwrap();
                            ds.execute(&query, &sess, None).await.unwrap();
                        }
                    }
                }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    // Issue #1470: a `tests/`-dir path is suppressed by `skip_in_test_dir`.
    // Gated run honours the production `applies_to_file` gate.
    #[test]
    fn allows_query_in_loop_in_tests_dir() {
        let src = "async fn f(ids: Vec<i32>) { for id in ids { db.query(id).await; } }";
        let diags =
            crate::rules::test_helpers::run_rule_gated(&Check, src, "crate/tests/signin.rs");
        assert!(diags.is_empty());
    }

    // Negative space: the same loop query in a production (non-test) path still
    // fires — the exemption is test-scoped, the rule still catches real N+1.
    #[test]
    fn flags_query_in_loop_in_production_path() {
        let src = "async fn f(ids: Vec<i32>) { for id in ids { db.query(id).await; } }";
        let diags =
            crate::rules::test_helpers::run_rule_gated(&Check, src, "crate/src/iam/signin.rs");
        assert_eq!(diags.len(), 1);
    }
}
