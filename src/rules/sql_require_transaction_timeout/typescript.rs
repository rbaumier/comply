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
    if ctx.file.path_segments.in_test_dir {
        return;
    }
    // File-level guard.
    if ctx.source_contains("statement_timeout") {
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
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

    #[test]
    fn no_fp_drizzle_in_test_file() {
        // Regression: drizzle() wrapping a proxied test connection — issue #546
        use crate::rules::file_ctx::{FileCtx, PathSegments};
        let src = r#"const legacyDb = drizzle({
  client: legacyClient,
  relations: legacySchema.relations,
});"#;
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..Default::default()
        };
        let diags = crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.ts", crate::project::default_static_project_ctx(), &file);
        assert!(diags.is_empty());
    }
}
