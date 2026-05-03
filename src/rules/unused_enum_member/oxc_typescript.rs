//! unused-enum-member OXC backend — flag TypeScript enum members declared
//! in the current file but never referenced anywhere within that file.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["enum"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // Map enum_name -> Vec<(member_name, line)>
        let mut enums: HashMap<String, Vec<(String, u32)>> = HashMap::new();
        // Set of (enum_name, member_name) that are referenced.
        let mut used: HashSet<(String, String)> = HashSet::new();
        // Track enum node IDs to skip their subtrees in usage collection.
        let mut enum_node_ids: HashSet<oxc_semantic::NodeId> = HashSet::new();

        // Pass 1: collect enum declarations (non-exported only).
        for node in semantic.nodes().iter() {
            let AstKind::TSEnumDeclaration(decl) = node.kind() else {
                continue;
            };

            // Skip exported enums.
            let nodes = semantic.nodes();
            let parent_id = nodes.parent_id(node.id());
            if parent_id != node.id() {
                let parent = nodes.get_node(parent_id);
                if matches!(parent.kind(), AstKind::ExportNamedDeclaration(_)) {
                    continue;
                }
            }
            // Also check if the source text starts with "export ".
            let decl_text =
                &ctx.source[decl.span.start as usize..decl.span.end as usize];
            if decl_text.starts_with("export ") {
                continue;
            }

            let enum_name = decl.id.name.as_str().to_string();
            let mut members = Vec::new();
            for member in &decl.body.members {
                let member_name =
                    &ctx.source[member.id.span().start as usize..member.id.span().end as usize];
                if member_name.is_empty() {
                    continue;
                }
                let (line, _) =
                    byte_offset_to_line_col(ctx.source, member.span.start as usize);
                members.push((member_name.to_string(), line as u32));
            }
            if !members.is_empty() {
                enums.insert(enum_name, members);
                enum_node_ids.insert(node.id());
            }
        }

        if enums.is_empty() {
            return diagnostics;
        }

        // Pass 2: collect usages (EnumName.MemberName patterns).
        for node in semantic.nodes().iter() {
            // Skip nodes inside enum declarations.
            let mut ancestor_id = node.id();
            let nodes = semantic.nodes();
            let mut skip = false;
            loop {
                if enum_node_ids.contains(&ancestor_id) {
                    skip = true;
                    break;
                }
                let parent_id = nodes.parent_id(ancestor_id);
                if parent_id == ancestor_id {
                    break;
                }
                ancestor_id = parent_id;
            }
            if skip {
                continue;
            }

            match node.kind() {
                AstKind::StaticMemberExpression(member) => {
                    if let Expression::Identifier(obj) = &member.object {
                        let obj_name = obj.name.as_str();
                        if enums.contains_key(obj_name) {
                            let prop_name = member.property.name.as_str();
                            used.insert((obj_name.to_string(), prop_name.to_string()));
                        }
                    }
                }
                AstKind::ComputedMemberExpression(member) => {
                    if let Expression::Identifier(obj) = &member.object {
                        let obj_name = obj.name.as_str();
                        if enums.contains_key(obj_name) {
                            if let Expression::StringLiteral(s) = &member.expression {
                                used.insert((
                                    obj_name.to_string(),
                                    s.value.as_str().to_string(),
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Diff: flag unused members.
        for (enum_name, members) in &enums {
            for (member_name, line) in members {
                if !used.contains(&(enum_name.clone(), member_name.clone())) {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: *line as usize,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "enum member `{enum_name}.{member_name}` is never referenced in this file."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}
