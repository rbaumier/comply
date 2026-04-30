//! sql-no-uuidv4-primary-key — Drizzle ORM backend.
//!
//! Flags Drizzle schema columns declared as `uuid('id').primaryKey()` whose
//! chain also calls `.defaultRandom()` or `.default(sql`gen_random_uuid()`)`.
//! UUIDv4 primary keys fragment B-tree indexes — prefer UUIDv7 or
//! `BIGINT GENERATED ALWAYS AS IDENTITY`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["call_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let Ok(name) = function.utf8_text(source_bytes) else {
            return;
        };
        if name != "uuid" {
            return;
        }

        let methods = collect_chain(node, source_bytes);
        let has_pk = methods.iter().any(|m| m == "primaryKey");
        if !has_pk {
            return;
        }
        let has_v4_default = methods
            .iter()
            .any(|m| m == "defaultRandom" || m == "default");
        if !has_v4_default {
            return;
        }

        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "UUIDv4 primary key fragments B-tree indexes — use \
                      UUIDv7 or `BIGINT GENERATED ALWAYS AS IDENTITY`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn collect_chain(start: tree_sitter::Node, source: &[u8]) -> Vec<String> {
    let mut methods = Vec::new();
    let mut current = start;
    while let Some(parent) = current.parent() {
        if parent.kind() == "member_expression"
            && parent.child_by_field_name("object").map(|o| o.id()) == Some(current.id())
        {
            let Some(grand) = parent.parent() else { break };
            if grand.kind() == "call_expression"
                && grand.child_by_field_name("function").map(|f| f.id()) == Some(parent.id())
            {
                if let Some(prop) = parent.child_by_field_name("property") {
                    methods.push(prop.utf8_text(source).unwrap_or("").to_string());
                }
                current = grand;
                continue;
            }
        }
        break;
    }
    methods
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_uuid_pk_default_random() {
        let src = "const id = uuid('id').primaryKey().defaultRandom();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_uuid_pk_default_sql() {
        let src = "const id = uuid('id').primaryKey().default(sql`gen_random_uuid()`);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_uuid_pk_without_default() {
        let src = "const id = uuid('id').primaryKey();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_uuid_default_without_pk() {
        let src = "const ref = uuid('ref_id').defaultRandom();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_serial_pk() {
        let src = "const id = serial('id').primaryKey();";
        assert!(run(src).is_empty());
    }
}
