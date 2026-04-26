//! better-auth-middleware-requires-headers — in `middleware.ts{,x}` files,
//! `getSession(...)` must be called with an object containing a `headers` key,
//! or Next.js middleware session lookup will fail.

use crate::diagnostic::{Diagnostic, Severity};

fn is_middleware_file(ctx: &crate::rules::backend::CheckCtx) -> bool {
    ctx.path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == "middleware.ts" || n == "middleware.tsx" || n == "middleware.js")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_middleware_file(ctx) {
        return;
    }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.utf8_text(source).unwrap_or("") != "getSession" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let obj = args.children(&mut cursor).find(|c| c.kind() == "object");

    let has_headers = match obj {
        Some(o) => {
            let mut c = o.walk();
            o.children(&mut c).any(|child| {
                if child.kind() != "pair" {
                    return false;
                }
                let Some(k) = child.child_by_field_name("key") else { return false };
                let key_text = k
                    .utf8_text(source)
                    .unwrap_or("")
                    .trim_matches(|c: char| c == '\'' || c == '"');
                key_text == "headers"
            })
        }
        None => false,
    };

    if has_headers {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`getSession()` in middleware must forward request headers — pass `{ headers: await headers() }` or session lookup will fail.".into(),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::{Diagnostic, Severity};
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run_at(path: &str, source: &str) -> Vec<Diagnostic> {
        let ctx = CheckCtx::for_test(Path::new(path), source);
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&ctx, &tree)
    }

    #[test]
    fn flags_get_session_no_args() {
        let d = run_at("middleware.ts", "const session = getSession()");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Error);
    }

    #[test]
    fn flags_get_session_without_headers() {
        assert_eq!(
            run_at(
                "middleware.ts",
                "const session = await getSession({ foo: 1 })"
            )
            .len(),
            1
        );
    }

    #[test]
    fn allows_get_session_with_headers() {
        assert!(run_at(
            "middleware.ts",
            "const session = await getSession({ headers: await headers() })"
        )
        .is_empty());
    }

    #[test]
    fn ignores_non_middleware_files() {
        assert!(run_at("api.ts", "const session = getSession()").is_empty());
    }

    #[test]
    fn allows_multiline_get_session_with_headers() {
        let src = "const session = await getSession({\n  headers: h,\n})";
        assert!(run_at("middleware.ts", src).is_empty());
    }
}
