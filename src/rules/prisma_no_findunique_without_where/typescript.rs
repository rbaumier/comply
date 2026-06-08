//! Flag `<x>.findUnique()` and `<x>.findUniqueOrThrow()` calls whose
//! argument object literal lacks a `where` key.

use crate::diagnostic::{Diagnostic, Severity};

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client") || crate::oxc_helpers::source_contains(source, "PrismaClient") || crate::oxc_helpers::source_contains(source, "prisma.")
}

fn object_has_where_key(obj: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = obj.walk();
    for child in obj.children(&mut cursor) {
        if child.kind() == "pair"
            && let Some(key) = child.child_by_field_name("key")
        {
            let text = std::str::from_utf8(&source[key.byte_range()]).unwrap_or("");
            if text == "where" || text == "\"where\"" || text == "'where'" {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_prisma_file(ctx.source) { return; }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" { return; }
    let Some(prop) = callee.child_by_field_name("property") else { return; };
    let prop_text = std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("");
    if !matches!(prop_text, "findUnique" | "findUniqueOrThrow") { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let mut cursor = args.walk();
    let arg_objs: Vec<_> = args.children(&mut cursor).filter(|c| c.kind() == "object").collect();
    if arg_objs.is_empty() {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            format!("`{prop_text}()` called without arguments — must include `{{ where: ... }}`."),
            Severity::Warning,
        ));
        return;
    }
    for obj in &arg_objs {
        if !object_has_where_key(*obj, source) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!("`{prop_text}()` argument is missing a `where` clause — call always resolves to null."),
                Severity::Warning,
            ));
            return;
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_find_unique_without_where() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { return prisma.user.findUnique({ select: { id: true } }); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_find_unique_with_where() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { return prisma.user.findUnique({ where: { id: 1 } }); }";
        assert!(run(src).is_empty());
    }
}
