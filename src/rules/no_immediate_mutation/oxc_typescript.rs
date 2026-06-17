//! OXC backend for no-immediate-mutation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ArrayExpressionElement, AssignmentTarget, Expression, Statement};
use std::sync::Arc;

pub struct Check;

/// Mutating methods on arrays that indicate immediate mutation.
const ARRAY_MUTATORS: &[&str] = &[
    "push",
    "unshift",
    "pop",
    "shift",
    "splice",
    "sort",
    "reverse",
    "fill",
    "copyWithin",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Walk all nodes looking for VariableDeclaration
        for node in semantic.nodes().iter() {
            let AstKind::VariableDeclaration(decl) = node.kind() else {
                continue;
            };

            // Only process declarations with exactly one declarator
            if decl.declarations.len() != 1 {
                continue;
            }

            let declarator = &decl.declarations[0];

            // Must have a simple identifier binding
            let oxc_ast::ast::BindingPattern::BindingIdentifier(ref id) = declarator.id else {
                continue;
            };
            let var_name = id.name.as_str();

            // Must have an initializer
            let Some(ref init) = declarator.init else {
                continue;
            };

            // Determine what kind of literal
            let literal_kind = classify_init(init, ctx.source);
            if literal_kind == LiteralKind::None {
                continue;
            }

            // Find the next sibling statement by looking at the parent
            let parent_id = semantic.nodes().parent_id(node.id());
            if parent_id == node.id() {
                continue;
            }
            let _parent = semantic.nodes().get_node(parent_id);

            // The parent should be something that contains statements
            // We need to find the next statement after this declaration
            let decl_end = decl.span.end;
            let Some(next_stmt_text) = find_next_statement_text(ctx.source, decl_end as usize) else {
                continue;
            };

            let next_stmt_text = next_stmt_text.trim();
            if next_stmt_text.is_empty() {
                continue;
            }

            let flagged = match literal_kind {
                LiteralKind::Array => {
                    // An array indexed/property assignment is only worth flagging
                    // when it could be inlined into the literal. It cannot when the
                    // initialiser is a spread copy (`[...x]`) — copy-then-set-one is
                    // the immutable-update idiom — or when the assignment target is a
                    // computed member with a non-literal key (`x[index]`), which has
                    // no array-literal syntax. Mutator methods stay flagged regardless.
                    let assignment_chainable = !array_init_has_spread(init)
                        && !next_assignment_is_computed_dynamic(semantic, node, var_name);
                    is_method_call_on_text(next_stmt_text, var_name, ARRAY_MUTATORS)
                        || (assignment_chainable
                            && is_property_assignment_text(next_stmt_text, var_name))
                }
                LiteralKind::Object => {
                    is_property_assignment_text(next_stmt_text, var_name)
                }
                LiteralKind::Set => {
                    is_method_call_on_text(next_stmt_text, var_name, &["add"])
                }
                LiteralKind::Map => {
                    is_method_call_on_text(next_stmt_text, var_name, &["set"])
                }
                LiteralKind::None => false,
            };

            if flagged {
                // Find position of next_stmt in source after decl_end
                let after_decl = &ctx.source[decl_end as usize..];
                let trimmed = after_decl.trim_start();
                let offset = decl_end as usize + (after_decl.len() - trimmed.len());
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "no-immediate-mutation".into(),
                    message: "Immediate mutation after variable assignment \u{2014} chain onto the initialiser instead.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

#[derive(PartialEq)]
enum LiteralKind {
    None,
    Array,
    Object,
    Set,
    Map,
}

fn classify_init(expr: &Expression, _source: &str) -> LiteralKind {
    match expr {
        Expression::ArrayExpression(_) => LiteralKind::Array,
        Expression::ObjectExpression(_) => LiteralKind::Object,
        Expression::NewExpression(new_expr) => {
            if let Expression::Identifier(id) = &new_expr.callee {
                match id.name.as_str() {
                    "Set" | "WeakSet" => LiteralKind::Set,
                    "Map" | "WeakMap" => LiteralKind::Map,
                    _ => LiteralKind::None,
                }
            } else {
                LiteralKind::None
            }
        }
        _ => LiteralKind::None,
    }
}

/// True when `init` is an array literal containing a spread element (`[...x]`).
/// A spread copy followed by a single-element set is the immutable-update idiom,
/// not a chainable builder.
fn array_init_has_spread(init: &Expression) -> bool {
    let Expression::ArrayExpression(arr) = init else {
        return false;
    };
    arr.elements
        .iter()
        .any(|el| matches!(el, ArrayExpressionElement::SpreadElement(_)))
}

/// True when the statement immediately after `decl_node` assigns to a computed
/// member of `var_name` with a non-static-literal key (`var_name[expr] = ...`,
/// where `expr` is not a numeric/string literal). A dynamic index can never be
/// inlined into an array literal.
fn next_assignment_is_computed_dynamic(
    semantic: &oxc_semantic::Semantic,
    decl_node: &oxc_semantic::AstNode,
    var_name: &str,
) -> bool {
    let nodes = semantic.nodes();
    let parent_id = nodes.parent_id(decl_node.id());
    if parent_id == decl_node.id() {
        return false;
    }
    let stmts: &oxc_allocator::Vec<Statement> = match nodes.kind(parent_id) {
        AstKind::FunctionBody(body) => &body.statements,
        AstKind::BlockStatement(block) => &block.body,
        AstKind::Program(program) => &program.body,
        _ => return false,
    };

    let decl_span = match decl_node.kind() {
        AstKind::VariableDeclaration(decl) => decl.span,
        _ => return false,
    };

    // The sibling immediately following the declaration.
    let mut found_self = false;
    for stmt in stmts.iter() {
        if found_self {
            return statement_is_computed_dynamic_assignment(stmt, var_name);
        }
        if let Statement::VariableDeclaration(d) = stmt
            && d.span == decl_span
        {
            found_self = true;
        }
    }
    false
}

/// True when `stmt` is `var_name[expr] = ...` with a non-literal key `expr`.
fn statement_is_computed_dynamic_assignment(stmt: &Statement, var_name: &str) -> bool {
    let Statement::ExpressionStatement(expr_stmt) = stmt else {
        return false;
    };
    let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
        return false;
    };
    let AssignmentTarget::ComputedMemberExpression(member) = &assign.left else {
        return false;
    };
    let Expression::Identifier(obj) = &member.object else {
        return false;
    };
    if obj.name.as_str() != var_name {
        return false;
    }
    // Static numeric/string keys could be inlined; dynamic keys cannot.
    !matches!(
        &member.expression,
        Expression::NumericLiteral(_) | Expression::StringLiteral(_)
    )
}

/// Find the text of the next statement after a given byte offset.
fn find_next_statement_text(source: &str, after: usize) -> Option<&str> {
    let rest = source.get(after..)?;
    let trimmed = rest.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    // Find end of statement (next semicolon or newline-terminated expression)
    let end = trimmed.find(';').map(|i| i + 1)
        .or_else(|| trimmed.find('\n'))
        .unwrap_or(trimmed.len());
    Some(&trimmed[..end])
}

/// Check if text looks like `varName.method(...)` where method is in the list.
fn is_method_call_on_text(stmt: &str, var_name: &str, methods: &[&str]) -> bool {
    for method in methods {
        let pattern = format!("{var_name}.{method}(");
        if stmt.starts_with(&pattern) {
            return true;
        }
    }
    false
}

/// Check if text looks like `varName.prop = ...` or `varName[...] = ...`.
fn is_property_assignment_text(stmt: &str, var_name: &str) -> bool {
    if !stmt.starts_with(var_name) {
        return false;
    }
    let rest = &stmt[var_name.len()..];
    if rest.starts_with('.') || rest.starts_with('[') {
        // Must have an assignment somewhere
        return rest.contains('=') && !rest.starts_with("==");
    }
    false
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // --- True positives that must keep flagging ---

    #[test]
    fn flags_array_sort() {
        assert_eq!(run_on("const arr = [3, 1, 2];\narr.sort();").len(), 1);
    }

    #[test]
    fn flags_array_push() {
        assert_eq!(run_on("const arr = [];\narr.push(1);").len(), 1);
    }

    // A spread-copy followed by a mutator method is still an immediate mutation —
    // the mutator path is unchanged by the spread-init exemption.
    #[test]
    fn flags_spread_copy_then_mutator() {
        assert_eq!(run_on("const a = [...x];\na.push(y);").len(), 1);
    }

    #[test]
    fn flags_object_property_assignment() {
        assert_eq!(run_on("const obj = {};\nobj.foo = 'bar';").len(), 1);
    }

    // An object built from a static literal with a static computed key is still
    // inlinable (`{ foo: 1 }`) — the object branch is untouched.
    #[test]
    fn flags_object_computed_static_key_assignment() {
        assert_eq!(run_on("const obj = {};\nobj['foo'] = 1;").len(), 1);
    }

    #[test]
    fn flags_set_add() {
        assert_eq!(run_on("const s = new Set();\ns.add(1);").len(), 1);
    }

    #[test]
    fn flags_map_set() {
        assert_eq!(run_on("const m = new Map();\nm.set('a', 1);").len(), 1);
    }

    // --- Regressions for #3933: array indexed-assignment that cannot be inlined ---

    // Spread copy + computed dynamic index (the canonical React immutable update).
    #[test]
    fn allows_spread_copy_dynamic_index_assignment() {
        let src = "const values = [...currentValue];\nvalues[index] = val;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Spread copy + static index: still exempt under the spread-init predicate
    // (copy-then-set-one is the immutable-update idiom, not a chainable builder).
    #[test]
    fn allows_spread_copy_static_index_assignment() {
        let src = "const v = [...x];\nv[0] = y;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // Static array literal + dynamic computed index: no array-literal syntax can
    // set element `i` inline, so the computed-index predicate exempts it.
    #[test]
    fn allows_static_array_dynamic_index_assignment() {
        let src = "const v = [0, 0, 0];\nv[i] = 1;";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // The exact #3933 shape: inside a function body (BlockStatement), spread copy
    // + computed dynamic index — the canonical React immutable single-element update.
    #[test]
    fn allows_spread_copy_dynamic_index_in_function_body() {
        let src = "function f(currentValue, index, val) {\n  const values = [...currentValue];\n  values[index] = val;\n  return values;\n}";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }
}
