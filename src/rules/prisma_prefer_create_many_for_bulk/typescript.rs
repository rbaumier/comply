//! Detect a `prisma.<model>.create()` call whose enclosing context is a
//! loop (for / for_in / for_of / while / do_while / forEach / map).

use crate::diagnostic::{Diagnostic, Severity};

fn is_prisma_file(source: &str) -> bool {
    source.contains("@prisma/client") || source.contains("PrismaClient") || source.contains("prisma.")
}

fn enclosing_is_loop(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "for_statement" | "for_in_statement" | "while_statement" | "do_statement" => return true,
            "call_expression" => {
                if let Some(callee) = parent.child_by_field_name("function")
                    && callee.kind() == "member_expression"
                    && let Some(prop) = callee.child_by_field_name("property")
                {
                    let text = std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("");
                    if matches!(text, "forEach" | "map") { return true; }
                }
            }
            _ => {}
        }
        cur = parent;
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_prisma_file(ctx.source) { return; }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" { return; }
    let Some(prop) = callee.child_by_field_name("property") else { return; };
    let prop_text = std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("");
    if prop_text != "create" { return; }
    if !enclosing_is_loop(node, source) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`create()` called inside a loop — use `createMany({ data: [...] })` for one round-trip.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_create_in_for_loop() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f(items: any[]) { for (const it of items) { await prisma.user.create({ data: it }); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_create_in_for_each() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f(items: any[]) { items.forEach(async (it) => { await prisma.user.create({ data: it }); }); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_single_create() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f(it: any) { await prisma.user.create({ data: it }); }";
        assert!(run(src).is_empty());
    }
}
