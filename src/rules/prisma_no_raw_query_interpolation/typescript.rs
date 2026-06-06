//! Flag `<x>.$queryRaw(...)` and `<x>.$executeRaw(...)` *call* forms — the
//! safe form is the tagged template literal `<x>.$queryRaw\`...\``.

use crate::diagnostic::{Diagnostic, Severity};

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client") || crate::oxc_helpers::source_contains(source, "PrismaClient") || crate::oxc_helpers::source_contains(source, "$queryRaw") || crate::oxc_helpers::source_contains(source, "$executeRaw")
}

crate::ast_check! { on ["call_expression"] prefilter = ["$queryRaw", "$executeRaw"] => |node, source, ctx, diagnostics|
    if !is_prisma_file(ctx.source) { return; }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" { return; }
    let Some(prop) = callee.child_by_field_name("property") else { return; };
    let prop_text = std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("");
    if !matches!(prop_text, "$queryRaw" | "$executeRaw") { return; }
    // Tagged-template form: tree-sitter exposes the template literal under
    // the `arguments` field with kind `template_string` — that's the safe
    // form, skip it. The unsafe call form has `arguments` of kind
    // `arguments` (a parenthesised list).
    if let Some(args) = node.child_by_field_name("arguments")
        && args.kind() == "template_string"
    {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("`{prop_text}(...)` accepts a string — concatenated input is SQL injection. Use the tagged-template form: `prisma.{prop_text}\\`SELECT ...\\``."),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_query_raw_call_form() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f(id: string) { return prisma.$queryRaw('SELECT * FROM u WHERE id = ' + id); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_tagged_template_form() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f(id: number) { return prisma.$queryRaw`SELECT * FROM u WHERE id = ${id}`; }";
        assert!(run(src).is_empty());
    }
}
