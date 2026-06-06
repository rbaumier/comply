//! api-list-requires-pagination — flag exported `GET` handlers when no
//! pagination primitive is referenced anywhere in the file.
//!
//! AST detection: walk the program node and look for `export_statement`
//! children whose inner declaration is named `GET` (either a function
//! declaration or a `const GET = ...` lexical_declaration). If the file
//! never mentions a pagination term, flag the handler declaration.

use crate::diagnostic::{Diagnostic, Severity};

const PAGINATION_TERMS: &[&str] = &["limit", "cursor", "page", "offset", "pageSize", "per_page"];

fn declaration_named_get<'a>(
    export_stmt: tree_sitter::Node<'a>,
    source: &[u8],
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = export_stmt.walk();
    for inner in export_stmt.children(&mut cursor) {
        match inner.kind() {
            "function_declaration" => {
                if let Some(name) = inner.child_by_field_name("name")
                    && name.utf8_text(source).unwrap_or("") == "GET"
                {
                    return Some(inner);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                let mut dc = inner.walk();
                for d in inner.children(&mut dc) {
                    if d.kind() == "variable_declarator"
                        && let Some(name) = d.child_by_field_name("name")
                        && name.utf8_text(source).unwrap_or("") == "GET"
                    {
                        return Some(inner);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    if PAGINATION_TERMS.iter().any(|p| ctx.source_contains(p)) {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "export_statement" {
            continue;
        }
        let Some(get_node) = declaration_named_get(child, source) else { continue };
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &get_node,
            super::META.id,
            "GET handler has no pagination — add `limit`/`cursor` or `page`/`pageSize` to prevent unbounded queries.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_get_without_pagination() {
        assert_eq!(
            run("export async function GET() { return db.select().from(users) }").len(),
            1
        );
    }

    #[test]
    fn allows_get_with_limit() {
        assert!(run(
            "export async function GET(req: Request) { const { limit } = await req.json(); return db.select().from(users).limit(limit) }"
        )
        .is_empty());
    }
}
