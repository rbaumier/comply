//! react-server-action-requires-auth — Server Actions performing
//! mutations (`.insert`/`.update`/`.delete`) must verify auth.
//!
//! AST detection: walk the program node, detect file-level
//! `"use server"` directive in the first few statements, then flag
//! every exported async `function_declaration` if the file performs
//! mutations and never calls a known auth helper.

use crate::diagnostic::{Diagnostic, Severity};

const AUTH_CALLS: &[&str] = &[
    "getSession(",
    "auth()",
    "verifySession",
    "requireAuth",
    "currentUser(",
];

const MUTATION_CALLS: &[&str] = &[".insert(", ".update(", ".delete("];

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

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

    let src = ctx.source;
    if !MUTATION_CALLS.iter().any(|c| src.contains(c)) {
        return;
    }
    if AUTH_CALLS.iter().any(|c| src.contains(c)) {
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
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &inner,
                super::META.id,
                "Server Action with mutations must verify authentication before proceeding.".into(),
                Severity::Warning,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_mutation_without_auth() {
        assert_eq!(
            run("'use server'\nexport async function create(t: string) { await db.insert(posts).values({ t }) }").len(),
            1
        );
    }

    #[test]
    fn allows_with_get_session() {
        assert!(run("'use server'\nexport async function create(t: string) { const s = await getSession(); await db.insert(posts).values({ t }) }").is_empty());
    }

    #[test]
    fn allows_read_only() {
        assert!(
            run("'use server'\nexport async function list() { return db.select().from(posts) }")
                .is_empty()
        );
    }

    #[test]
    fn allows_non_server_file() {
        assert!(
            run("export async function create(t: string) { await db.insert(posts).values({ t }) }")
                .is_empty()
        );
    }
}
