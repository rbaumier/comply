//! consistent-destructuring OXC backend.
//!
//! Flags member expressions like `user.age` when the same object was
//! already destructured earlier in the same scope. When both the
//! destructuring source and the member-access object resolve to a binding,
//! matching is keyed on that binding's symbol, so two identically named
//! variables in unrelated scopes never cross-trigger.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_semantic::SymbolId;
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
        struct Destructured {
            obj_text: String,
            obj_symbol: Option<SymbolId>,
            end_byte: u32,
            fn_range: Option<(u32, u32)>,
        }
        let mut destructured: Vec<Destructured> = Vec::new();

        // Phase 2: collect all member expression candidates
        struct Candidate {
            obj_text: String,
            obj_symbol: Option<SymbolId>,
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

                    let fn_range = {
                        let mut result = None;
                        for ancestor in nodes.ancestors(node.id()) {
                            match ancestor.kind() {
                                AstKind::Function(f) => {
                                    result = Some((f.span.start, f.span.end));
                                    break;
                                }
                                AstKind::ArrowFunctionExpression(a) => {
                                    result = Some((a.span.start, a.span.end));
                                    break;
                                }
                                _ => {}
                            }
                        }
                        result
                    };
                    let obj_symbol = identifier_symbol(init, semantic);
                    destructured.push(Destructured {
                        obj_text: obj_text.to_string(),
                        obj_symbol,
                        end_byte: decl.span.end,
                        fn_range,
                    });
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
                    let obj_symbol = identifier_symbol(&member.object, semantic);

                    candidates.push(Candidate {
                        obj_text: obj_text.to_string(),
                        obj_symbol,
                        prop_text: prop_text.to_string(),
                        start_byte: member.span().start,
                    });
                }
                _ => {}
            }
        }

        // Phase 3: match candidates against destructured objects
        for c in &candidates {
            for d in &destructured {
                if c.obj_text == d.obj_text && c.start_byte > d.end_byte {
                    // When both sides resolve to a binding, they must be the
                    // *same* binding. This rejects identically named variables
                    // declared in unrelated scopes (e.g. a loop variable and a
                    // sibling callback parameter that happen to share a name).
                    if let (Some(cand_sym), Some(decl_sym)) = (c.obj_symbol, d.obj_symbol)
                        && cand_sym != decl_sym
                    {
                        continue;
                    }
                    let scope_ok = match d.fn_range {
                        None => true,
                        Some((fn_start, fn_end)) => {
                            c.start_byte >= fn_start && c.start_byte <= fn_end
                        }
                    };
                    if scope_ok {
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
        }

        diagnostics
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn skips_cross_function_scope() {
        let code = r#"
            const obj = { x: 1, y: 2 };
            function first() {
                const { x } = obj;
                console.log(x);
            }
            function second() {
                console.log(obj.y);
            }
        "#;
        assert!(run(code).is_empty(), "Should not flag across function scopes");
    }

    #[test]
    fn flags_same_function_scope() {
        let code = r#"
            function test() {
                const { x } = obj;
                console.log(x);
                console.log(obj.y);
            }
        "#;
        let diags = run(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains('y'));
    }

    #[test]
    fn flags_nested_inner_scope() {
        let code = r#"
            function outer() {
                const { x } = obj;
                function inner() {
                    console.log(obj.y);
                }
            }
        "#;
        let diags = run(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains('y'));
    }

    #[test]
    fn skips_same_name_different_scopes() {
        // Issue #1224: a loop variable and a sibling callback parameter share
        // the name `sponsor` but are distinct bindings. Destructuring the loop
        // variable must not flag member access on the callback parameter.
        let code = r#"
            for (const sponsor of sponsors) {
                const { login } = sponsor;
                console.log(login);
            }
            const cols = buckets.map((sponsor) => {
                const imgSrc = new URL(sponsor.imgSrc);
                return sponsor.link;
            });
        "#;
        assert!(
            run(code).is_empty(),
            "Should not flag member access on a same-named binding in an unrelated scope"
        );
    }

    #[test]
    fn flags_same_binding_in_same_scope() {
        // Negative-space guard: a genuine within-scope inconsistency on the
        // very same binding is still flagged.
        let code = r#"
            function handle(sponsor) {
                const { login } = sponsor;
                console.log(login);
                console.log(sponsor.imgSrc);
            }
        "#;
        let diags = run(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("imgSrc"));
    }
}

/// Resolve `expr` to the symbol of its binding when it is a plain identifier
/// reference. Returns `None` for `this`, member chains, or unresolved globals,
/// which have no single binding to key matching on.
fn identifier_symbol(expr: &Expression, semantic: &oxc_semantic::Semantic) -> Option<SymbolId> {
    let Expression::Identifier(ident) = expr else {
        return None;
    };
    let ref_id = ident.reference_id.get()?;
    semantic.scoping().get_reference(ref_id).symbol_id()
}

fn is_simple_expression(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(_) | Expression::ThisExpression(_) => true,
        Expression::StaticMemberExpression(mem) => is_simple_expression(&mem.object),
        _ => false,
    }
}
