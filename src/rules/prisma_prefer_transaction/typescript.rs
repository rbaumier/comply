//! Count Prisma write calls (`create`/`update`/`delete`/`upsert` and their
//! `*Many` variants) per function body. If two or more appear without a
//! surrounding `$transaction`, flag the function.

use crate::diagnostic::{Diagnostic, Severity};

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client") || crate::oxc_helpers::source_contains(source, "PrismaClient") || crate::oxc_helpers::source_contains(source, "prisma.")
}

const WRITE_METHODS: &[&str] = &[
    "create", "createMany", "update", "updateMany", "delete", "deleteMany", "upsert",
];

fn count_writes(node: tree_sitter::Node, source: &[u8]) -> usize {
    let mut total = 0usize;
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression"
            && let Some(callee) = n.child_by_field_name("function")
            && callee.kind() == "member_expression"
            && let Some(prop) = callee.child_by_field_name("property")
        {
            let text = std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("");
            if WRITE_METHODS.contains(&text) {
                total += 1;
            }
        }
        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    total
}

fn body_uses_transaction(node: tree_sitter::Node, source: &[u8]) -> bool {
    let text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
    text.contains("$transaction")
}

crate::ast_check! { on ["function_declaration", "method_definition", "arrow_function", "function_expression"] => |node, source, ctx, diagnostics|
    if !is_prisma_file(ctx.source) { return; }
    let Some(body) = node.child_by_field_name("body") else { return; };
    if body_uses_transaction(body, source) { return; }
    let writes = count_writes(body, source);
    if writes < 2 { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("{writes} Prisma write calls in this function — wrap them in `prisma.$transaction([...])` for atomicity."),
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
    fn flags_two_writes_no_transaction() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { await prisma.user.create({ data: {} }); await prisma.post.create({ data: {} }); }";
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_writes_in_transaction() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { await prisma.$transaction([prisma.user.create({ data: {} }), prisma.post.create({ data: {} })]); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_single_write() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { await prisma.user.create({ data: {} }); }";
        assert!(run(src).is_empty());
    }
}
