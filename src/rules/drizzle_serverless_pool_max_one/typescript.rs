//! Flag `new Pool({...})` in serverless contexts (path or file content
//! suggests Edge/Lambda) where `max` is not set to `1`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_serverless(path: &std::path::Path, source: &str) -> bool {
    let p = path.to_string_lossy();
    let path_hint = p.contains("/api/")
        || p.contains("lambda")
        || p.contains("edge")
        || p.contains("worker")
        || p.contains("functions/")
        || p.contains("netlify")
        || p.contains("cloudflare");
    let source_hint = source.contains("runtime = 'edge'")
        || source.contains("runtime: 'edge'")
        || source.contains("\"runtime\": \"edge\"")
        || source.contains("AWSLambda")
        || source.contains("APIGatewayProxyHandler")
        || source.contains("export const runtime");
    path_hint || source_hint
}

fn constructor_is_pool<'a>(node: &tree_sitter::Node<'a>, src: &'a [u8]) -> bool {
    let Some(ctor) = node.child_by_field_name("constructor") else {
        return false;
    };
    ctor.utf8_text(src).unwrap_or("") == "Pool"
}

fn first_object_arg<'a>(node: &tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let args = node.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    args.children(&mut cursor).find(|&c| c.kind() == "object")
}

fn max_is_one(obj: tree_sitter::Node<'_>, src: &[u8]) -> bool {
    let mut cursor = obj.walk();
    for child in obj.children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let Some(k) = child.child_by_field_name("key") else {
            continue;
        };
        let kt = k.utf8_text(src).unwrap_or("").trim_matches(['"', '\'']);
        if kt == "max" {
            let Some(v) = child.child_by_field_name("value") else {
                continue;
            };
            return v.utf8_text(src).unwrap_or("").trim() == "1";
        }
    }
    false
}

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
    if !constructor_is_pool(&node, source) {
        return;
    }
    if !is_serverless(ctx.path, ctx.source) {
        return;
    }
    let has_max_one = first_object_arg(&node)
        .map(|obj| max_is_one(obj, source))
        .unwrap_or(false);
    if has_max_one {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Serverless `new Pool()` must set `max: 1` — each invocation has its own pool, >1 multiplies DB connections with concurrency.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(src, &Check, "app/api/users/route.ts")
    }

    fn run_non_serverless(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(src, &Check, "src/db.ts")
    }

    #[test]
    fn flags_pool_without_max_one_in_api() {
        let src = "const pool = new Pool({ connectionString: 'x' })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_pool_with_max_one_in_api() {
        let src = "const pool = new Pool({ connectionString: 'x', max: 1 })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_serverless() {
        let src = "const pool = new Pool({ connectionString: 'x' })";
        assert!(run_non_serverless(src).is_empty());
    }
}
