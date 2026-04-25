//! zod-prefer-safe-parse backend — flag `.parse()` calls inside route
//! handler files. Only fires when the file looks like a route handler
//! (filename heuristic OR an exported HTTP-verb function is present in
//! the source).

use crate::diagnostic::{Diagnostic, Severity};

const ROUTE_FILE_PATTERNS: &[&str] = &[
    "route.ts",
    "route.tsx",
    "handler.ts",
    "+server.ts",
    "page.server.ts",
    "controller.ts",
];

fn is_route_file(ctx: &crate::rules::backend::CheckCtx) -> bool {
    let file_name = ctx.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if ROUTE_FILE_PATTERNS.iter().any(|p| file_name.ends_with(p)) {
        return true;
    }
    let src = ctx.source;
    src.contains("export async function GET")
        || src.contains("export async function POST")
        || src.contains("export async function PUT")
        || src.contains("export async function DELETE")
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    if !is_route_file(ctx) { return; }

    let Some(function) = node.child_by_field_name("function") else { return };
    if function.kind() != "member_expression" { return; }

    let Some(property) = function.child_by_field_name("property") else { return };
    let Ok(method) = property.utf8_text(source) else { return };
    if method != "parse" { return; }

    // Skip JSON.parse(...) — receiver is the identifier `JSON`.
    if let Some(object) = function.child_by_field_name("object")
        && object.kind() == "identifier"
        && object.utf8_text(source).ok() == Some("JSON")
    {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Use `.safeParse()` in route handlers — `.parse()` throws `ZodError` which leaks schema internals to clients.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(path: &str, src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(src, &Check, path)
    }

    #[test]
    fn flags_parse_in_route() {
        assert_eq!(
            run(
                "route.ts",
                "export async function POST() { const body = schema.parse(data) }"
            )
            .len(),
            1
        );
    }

    #[test]
    fn allows_safe_parse() {
        assert!(run("route.ts", "const r = schema.safeParse(data)").is_empty());
    }

    #[test]
    fn allows_json_parse() {
        assert!(
            run(
                "route.ts",
                "export async function POST() { const body = JSON.parse(raw) }"
            )
            .is_empty()
        );
    }

    #[test]
    fn ignores_non_route() {
        assert!(run("utils.ts", "const x = schema.parse(data)").is_empty());
    }
}
