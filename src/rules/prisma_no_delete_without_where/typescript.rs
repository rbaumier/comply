//! Flag `<x>.deleteMany()` calls without `where`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client") || crate::oxc_helpers::source_contains(source, "PrismaClient") || crate::oxc_helpers::source_contains(source, "prisma.")
}

fn object_has_key(obj: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut cursor = obj.walk();
    for child in obj.children(&mut cursor) {
        if child.kind() == "pair"
            && let Some(key) = child.child_by_field_name("key")
        {
            let text = std::str::from_utf8(&source[key.byte_range()]).unwrap_or("");
            if text == name { return true; }
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
    if prop_text != "deleteMany" { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let mut cursor = args.walk();
    let arg_objs: Vec<_> = args.children(&mut cursor).filter(|c| c.kind() == "object").collect();
    if arg_objs.is_empty() {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "`deleteMany()` with no arguments deletes every row in the table.".into(),
            Severity::Error,
        ));
        return;
    }
    for obj in arg_objs {
        if !object_has_key(obj, source, "where") {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                "`deleteMany()` without `where` deletes every row in the table.".into(),
                Severity::Error,
            ));
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_delete_many_no_args() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { return prisma.user.deleteMany(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_many_without_where() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { return prisma.user.deleteMany({}); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_delete_many_with_where() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { return prisma.user.deleteMany({ where: { active: false } }); }";
        assert!(run(src).is_empty());
    }
}
