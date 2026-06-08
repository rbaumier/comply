//! react-server-action-requires-validation — Server Actions with
//! parameters must validate input via `.parse()`/`.safeParse()` (or
//! tRPC `.input()`).
//!
//! AST detection: walk the program node, detect file-level
//! `"use server"` directive, then flag every exported `async function`
//! that takes parameters when the file never calls a known validator.

use crate::diagnostic::{Diagnostic, Severity};

const VALIDATOR_CALLS: &[&str] = &[".parse(", ".safeParse(", ".input("];

fn is_use_server_directive(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "expression_statement" {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "string" {
            let text = child.utf8_text(source).unwrap_or("");
            let inner = text.trim_matches(|c| c == '"' || c == '\'' || c == ';');
            if inner == "use server" {
                return true;
            }
        }
    }
    false
}

fn has_parameters(func: tree_sitter::Node) -> bool {
    let Some(params) = func.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = params.walk();
    params.named_children(&mut cursor).any(|_| true)
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let src = ctx.source;
    if VALIDATOR_CALLS.iter().any(|c| src.contains(c)) {
        return;
    }

    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    let has_use_server = children
        .iter()
        .take(5)
        .any(|c| is_use_server_directive(*c, source));
    if !has_use_server {
        return;
    }

    for child in &children {
        if child.kind() != "export_statement" {
            continue;
        }
        let mut ec = child.walk();
        for inner in child.children(&mut ec) {
            if inner.kind() != "function_declaration" {
                continue;
            }
            let text = inner.utf8_text(source).unwrap_or("");
            if !text.starts_with("async ") {
                continue;
            }
            if !has_parameters(inner) {
                continue;
            }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &inner,
                super::META.id,
                "Server Action with parameters must validate input with `.parse()` or `.safeParse()`.".into(),
                Severity::Warning,
            ));
        }
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_params_no_parse() {
        assert_eq!(
            run("'use server'\nexport async function del(id: string) { await db.delete(x) }").len(),
            1
        );
    }

    #[test]
    fn allows_with_parse() {
        assert!(
            run("'use server'\nexport async function del(input: unknown) { schema.parse(input); }")
                .is_empty()
        );
    }

    #[test]
    fn allows_no_params() {
        assert!(
            run("'use server'\nexport async function list() { return db.select() }").is_empty()
        );
    }

    #[test]
    fn allows_non_server_file() {
        assert!(run("export async function del(id: string) { await db.delete(x) }").is_empty());
    }
}
