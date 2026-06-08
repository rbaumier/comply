//! Flag `canActivate` methods whose declared return type is not `boolean`,
//! `Promise<boolean>`, or `Observable<boolean>`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/") || crate::oxc_helpers::source_contains(source, "CanActivate")
}

fn return_type_text<'a>(method: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let ret = method.child_by_field_name("return_type")?;
    std::str::from_utf8(&source[ret.byte_range()]).ok()
}

crate::ast_check! { on ["method_definition"] => |node, source, ctx, diagnostics|
    if !is_nestjs_file(ctx.source) { return; }
    let Some(name_node) = node.child_by_field_name("name") else { return; };
    let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
    if name != "canActivate" { return; }
    let Some(rt) = return_type_text(node, source) else {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &name_node,
            super::META.id,
            "`canActivate` is missing an explicit return type — must be `boolean | Promise<boolean> | Observable<boolean>`.".into(),
            Severity::Warning,
        ));
        return;
    };
    if rt.contains("boolean") { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &name_node,
        super::META.id,
        format!("`canActivate` return type `{rt}` should resolve to `boolean`."),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_void_return() {
        let src = "import { CanActivate } from '@nestjs/common';\nclass G implements CanActivate { canActivate(): void {} }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_boolean_return() {
        let src = "import { CanActivate } from '@nestjs/common';\nclass G implements CanActivate { canActivate(): boolean { return true; } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_promise_boolean_return() {
        let src = "import { CanActivate } from '@nestjs/common';\nclass G implements CanActivate { async canActivate(): Promise<boolean> { return true; } }";
        assert!(run(src).is_empty());
    }
}
