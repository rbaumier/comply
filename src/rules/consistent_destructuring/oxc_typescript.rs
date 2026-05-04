//! consistent-destructuring OXC backend.
//!
//! Flags member expressions like `user.age` when the same object was
//! already destructured earlier in the same scope.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let source = ctx.source;
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        // Phase 1: collect all destructuring declarations
        // (object_text, end_byte, has_rest)
        let mut destructured: Vec<(String, u32, bool)> = Vec::new();

        // Phase 2: collect all member expression candidates
        struct Candidate {
            obj_text: String,
            prop_text: String,
            start_byte: u32,
        }
        let mut candidates: Vec<Candidate> = Vec::new();

        for node in nodes.iter() {
            match node.kind() {
                AstKind::VariableDeclarator(decl) => {
                    let BindingPattern::ObjectPattern(pattern) = &decl.id else {
                        continue;
                    };
                    let Some(init) = &decl.init else { continue };
                    if !is_simple_expression(init) {
                        continue;
                    }
                    let obj_text = &source[init.span().start as usize..init.span().end as usize];

                    let has_rest = pattern.rest.is_some();
                    if has_rest {
                        continue;
                    }

                    let mut has_props = false;
                    for prop in &pattern.properties {
                        if let PropertyKey::StaticIdentifier(_) = &prop.key { has_props = true; }
                    }
                    if !has_props {
                        continue;
                    }

                    destructured.push((obj_text.to_string(), decl.span.end, has_rest));
                }
                AstKind::StaticMemberExpression(member) => {
                    // Skip if parent is a member expression (nested: user.address.city)
                    let parent_id = nodes.parent_id(node.id());
                    if parent_id != node.id() {
                        let parent = nodes.get_node(parent_id);
                        match parent.kind() {
                            AstKind::StaticMemberExpression(parent_mem) => {
                                // If we are the object of the parent, skip (nested access)
                                if parent_mem.object.span() == member.span() {
                                    continue;
                                }
                            }
                            AstKind::CallExpression(call) => {
                                // Skip if this is the callee of a call (user.greet())
                                if call.callee.span() == member.span() {
                                    continue;
                                }
                            }
                            AstKind::AssignmentExpression(assign) => {
                                // Skip assignments (user.age = 5)
                                if assign.left.span().start == member.span().start
                                    && assign.left.span().end == member.span().end
                                {
                                    continue;
                                }
                            }
                            // Check grandparent for augmented assignment
                            _ => {
                                // Walk up further to check for augmented assignment
                                let gp_id = nodes.parent_id(parent_id);
                                if gp_id != parent_id {
                                    let gp = nodes.get_node(gp_id);
                                    if let AstKind::AssignmentExpression(assign) = gp.kind()
                                        && assign.left.span().start == member.span().start
                                            && assign.left.span().end == member.span().end
                                        {
                                            continue;
                                        }
                                }
                            }
                        }
                    }

                    let obj_text = &source[member.object.span().start as usize..member.object.span().end as usize];
                    let prop_text = member.property.name.as_str();

                    candidates.push(Candidate {
                        obj_text: obj_text.to_string(),
                        prop_text: prop_text.to_string(),
                        start_byte: member.span().start,
                    });
                }
                _ => {}
            }
        }

        // Phase 3: match candidates against destructured objects
        for c in &candidates {
            for (decl_obj, decl_end, _) in &destructured {
                if c.obj_text == *decl_obj && c.start_byte > *decl_end {
                    let (line, column) = byte_offset_to_line_col(source, c.start_byte as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: "consistent-destructuring".into(),
                        message: format!(
                            "Use destructured variable for `{}` instead of `{}.{}`.",
                            c.prop_text, c.obj_text, c.prop_text,
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
            }
        }

        diagnostics
    }
}

fn is_simple_expression(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(_) | Expression::ThisExpression(_) => true,
        Expression::StaticMemberExpression(mem) => is_simple_expression(&mem.object),
        _ => false,
    }
}
