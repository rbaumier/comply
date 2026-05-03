//! drizzle-zod-prefer-generated-schema OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const TABLE_FNS: &[&str] = &["pgTable", "mysqlTable", "sqliteTable", "table"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["drizzle-zod", "createInsertSchema", "createSelectSchema"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut has_drizzle_import = false;
        let mut has_zod_import = false;
        let mut has_table_call = false;
        let mut uses_generator = false;
        let mut z_object_spans = Vec::new();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::ImportDeclaration(import) => {
                    let src = import.source.value.as_str();
                    if src == "drizzle-orm"
                        || src.starts_with("drizzle-orm/")
                        || src == "drizzle-zod"
                    {
                        has_drizzle_import = true;
                    }
                    if src == "zod" {
                        has_zod_import = true;
                    }
                }
                AstKind::CallExpression(call) => {
                    match &call.callee {
                        Expression::Identifier(id) => {
                            let name = id.name.as_str();
                            if TABLE_FNS.contains(&name) {
                                has_table_call = true;
                            }
                            if name == "createInsertSchema" || name == "createSelectSchema" {
                                uses_generator = true;
                            }
                        }
                        Expression::StaticMemberExpression(member) => {
                            if let Expression::Identifier(obj) = &member.object {
                                if obj.name.as_str() == "z"
                                    && member.property.name.as_str() == "object"
                                {
                                    z_object_spans.push(call.span);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        if !has_drizzle_import || !has_zod_import || !has_table_call || uses_generator {
            return Vec::new();
        }

        z_object_spans
            .iter()
            .map(|span| {
                let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Manual `z.object({})` in a Drizzle schema file likely duplicates column definitions — use `createInsertSchema`/`createSelectSchema` from `drizzle-zod` instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}
