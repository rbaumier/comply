//! tanstack-start-server-fn-file-convention backend — flag the first
//! `createServerFn(...)` call expression in a file whose name does not
//! end in `.functions.ts(x)`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_functions_file(ctx: &crate::rules::backend::CheckCtx) -> bool {
    let file_name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    file_name.ends_with(".functions.ts") || file_name.ends_with(".functions.tsx")
}

#[derive(Debug)]
pub struct Check;

impl crate::rules::backend::AstCheck for Check {
    fn check(
        &self,
        ctx: &crate::rules::backend::CheckCtx,
        tree: &tree_sitter::Tree,
    ) -> Vec<Diagnostic> {
        if is_functions_file(ctx) {
            return Vec::new();
        }
        let source = ctx.source.as_bytes();
        let file_name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let mut diags: Vec<Diagnostic> = Vec::new();
        crate::rules::walker::walk_tree(tree, |node| {
            if !diags.is_empty() {
                return;
            }
            if node.kind() != "call_expression" {
                return;
            }
            let Some(function) = node.child_by_field_name("function") else {
                return;
            };
            // Match either `createServerFn(...)` directly or
            // `createServerFn().method(...)` chains: the bare identifier
            // appears as the function child of the inner call_expression,
            // which we'll visit independently anyway.
            let matches = match function.kind() {
                "identifier" => function.utf8_text(source).ok() == Some("createServerFn"),
                _ => false,
            };
            if !matches {
                return;
            }
            diags.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!("`createServerFn` must be in a `*.functions.ts` file, not `{file_name}`."),
                Severity::Warning,
            ));
        });
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(path: &str, src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(src, &Check, path)
    }

    #[test]
    fn flags_wrong_file_name() {
        assert_eq!(
            run("src/users/actions.ts", "const fn = createServerFn()").len(),
            1
        );
    }

    #[test]
    fn allows_functions_ts() {
        assert!(
            run(
                "src/users/users.functions.ts",
                "const fn = createServerFn()"
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_no_server_fn() {
        assert!(run("src/users/actions.ts", "const x = 1").is_empty());
    }
}
