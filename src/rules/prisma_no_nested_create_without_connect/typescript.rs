//! Detect a `create:` value object inside a `data:` value object inside an
//! outer `data:` — i.e. nested-create — that does NOT also include a
//! `connect:` key (which would imply linking to an existing record).

use crate::diagnostic::{Diagnostic, Severity};

fn is_prisma_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@prisma/client") || crate::oxc_helpers::source_contains(source, "PrismaClient") || crate::oxc_helpers::source_contains(source, "prisma.")
}

fn pair_key_is(pair: tree_sitter::Node, source: &[u8], expected: &str) -> bool {
    if let Some(key) = pair.child_by_field_name("key") {
        let text = std::str::from_utf8(&source[key.byte_range()]).unwrap_or("");
        return text == expected || text.trim_matches('"').trim_matches('\'') == expected;
    }
    false
}

/// Returns true if any pair in the object has key `name`.
fn object_has_key(obj: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut cursor = obj.walk();
    for child in obj.children(&mut cursor) {
        if child.kind() == "pair" && pair_key_is(child, source, name) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    if !is_prisma_file(ctx.source) { return; }
    // Looking for `<relation>: { create: { ... } }` nested inside an outer `data:`.
    let Some(value) = node.child_by_field_name("value") else { return; };
    if value.kind() != "object" { return; }
    if !object_has_key(value, source, "create") { return; }
    if object_has_key(value, source, "connect") { return; }
    // Outer key must NOT itself be `data` — that's the top-level data object.
    let Some(key) = node.child_by_field_name("key") else { return; };
    let key_text = std::str::from_utf8(&source[key.byte_range()]).unwrap_or("");
    if key_text == "data" || key_text == "where" || key_text == "select" || key_text == "include" { return; }
    // Confirm that the enclosing pair (going up) is `data:` — this means we're in a Prisma write.
    let mut cur = node;
    let mut found_data = false;
    while let Some(parent) = cur.parent() {
        if parent.kind() == "pair"
            && pair_key_is(parent, source, "data")
        {
            found_data = true;
            break;
        }
        cur = parent;
    }
    if !found_data { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Nested `create` for `{key_text}` without `connect` — children may orphan on rollback. Use `connect: {{ id }}` or split into a `$transaction`."),
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
    fn flags_nested_create_without_connect() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { await prisma.user.create({ data: { name: 'a', posts: { create: { title: 't' } } } }); }";
        assert!(!run(src).is_empty());
    }

    #[test]
    fn allows_nested_create_with_connect() {
        let src = "import { PrismaClient } from '@prisma/client';\nconst prisma = new PrismaClient();\nasync function f() { await prisma.user.create({ data: { name: 'a', org: { connect: { id: 1 }, create: { name: 'x' } } } }); }";
        assert!(run(src).is_empty());
    }
}
