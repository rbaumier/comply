//! prisma-no-nested-create-without-connect oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

fn prop_key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

/// Returns true if the object expression has a property with the given key.
fn object_has_key<'a>(
    props: &oxc_allocator::Vec<'a, ObjectPropertyKind<'a>>,
    name: &str,
) -> bool {
    props.iter().any(|p| {
        if let ObjectPropertyKind::ObjectProperty(prop) = p {
            prop_key_name(&prop.key) == Some(name)
        } else {
            false
        }
    })
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@prisma/client", "PrismaClient", "prisma."])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };

        // Get the key name. Skip top-level structural keys.
        let Some(key_text) = prop_key_name(&prop.key) else {
            return;
        };
        if matches!(key_text, "data" | "where" | "select" | "include") {
            return;
        }

        // Value must be an object expression containing `create` but not `connect`.
        let oxc_ast::ast::Expression::ObjectExpression(obj) = &prop.value else {
            return;
        };
        if !object_has_key(&obj.properties, "create") {
            return;
        }
        if object_has_key(&obj.properties, "connect") {
            return;
        }

        // Walk ancestors to find an enclosing ObjectProperty with key `data`.
        let mut found_data = false;
        let mut first = true;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if first {
                first = false;
                continue;
            }
            if let AstKind::ObjectProperty(parent_prop) = ancestor.kind() {
                if prop_key_name(&parent_prop.key) == Some("data") {
                    found_data = true;
                    break;
                }
            }
        }
        if !found_data {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Nested `create` for `{key_text}` without `connect` — children may orphan on rollback. Use `connect: {{ id }}` or split into a `$transaction`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
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
