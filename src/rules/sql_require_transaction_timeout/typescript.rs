//! sql-require-transaction-timeout — flag `new Pool(...)`, `drizzle(...)`,
//! and `createPool(...)` calls when the file never references
//! `statement_timeout`.
//!
//! AST detection: walk `call_expression` and `new_expression` nodes,
//! match the callee name, and (file-level prefilter) check that
//! `statement_timeout` doesn't appear anywhere in the source.

use crate::diagnostic::{Diagnostic, Severity};

fn callee_name<'a>(call: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    call.child_by_field_name("function")
        .or_else(|| call.child_by_field_name("constructor"))
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // File-level guard.
    if ctx.source.contains("statement_timeout") {
        return;
    }
    let kind = node.kind();
    if kind != "new_expression" && kind != "call_expression" {
        return;
    }
    let name = callee_name(node, source);
    let matches = match kind {
        "new_expression" => name == "Pool",
        "call_expression" => name == "drizzle" || name == "createPool",
        _ => false,
    };
    if !matches {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "DB pool config is missing `statement_timeout` — add it to prevent runaway queries.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_pool_without_timeout() {
        assert_eq!(
            run("const pool = new Pool({ connectionString: url })").len(),
            1
        );
    }

    #[test]
    fn allows_pool_with_timeout() {
        assert!(
            run("const pool = new Pool({ connectionString: url, statement_timeout: '30s' })")
                .is_empty()
        );
    }

    #[test]
    fn ignores_non_pool_files() {
        assert!(run("const x = 1;").is_empty());
    }
}
