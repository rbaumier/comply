//! Flag `new Response(JSON.stringify(...))`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["new_expression"] prefilter = ["@tanstack/start", "@tanstack/react-start"] => |node, source, ctx, diagnostics|
    let Some(ctor) = node.child_by_field_name("constructor") else { return; };
    let Ok(ctor_name) = ctor.utf8_text(source) else { return; };
    if ctor_name != "Response" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    if !first_arg_is_json_stringify(args, source) { return; }

    // File must use TanStack Start, where `json()` is importable.
    if !ctx.source_contains("@tanstack/start") && !ctx.source_contains("@tanstack/react-start") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Use `json(data)` from `@tanstack/react-start` instead of \
         `new Response(JSON.stringify(data))`."
            .into(),
        Severity::Warning,
    ));
}

fn first_arg_is_json_stringify(args: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let Some(first) = args.named_child(0) else {
        return false;
    };
    if first.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = first.child_by_field_name("function") else {
        return false;
    };
    let Ok(name) = callee.utf8_text(source) else {
        return false;
    };
    name == "JSON.stringify"
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    const TANSTACK_IMPORT: &str = "import { json } from '@tanstack/react-start';\n";

    #[test]
    fn flags_new_response_json_stringify() {
        let src = format!("{TANSTACK_IMPORT}return new Response(JSON.stringify({{ ok: true }}));");
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn flags_with_headers_opts() {
        let src = format!(
            "{TANSTACK_IMPORT}return new Response(JSON.stringify(data), {{ headers: {{ 'content-type': 'application/json' }} }});"
        );
        assert_eq!(run(&src).len(), 1);
    }

    #[test]
    fn allows_json_helper() {
        let src = format!("{TANSTACK_IMPORT}return json({{ ok: true }});");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn allows_new_response_text() {
        let src = format!("{TANSTACK_IMPORT}return new Response('hello');");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn ignores_when_no_tanstack_start_import() {
        let src = "const mockResponse = new Response(JSON.stringify(body), { status: 400, headers: { 'Content-Type': 'application/problem+json' } });";
        assert!(run(src).is_empty());
    }
}
