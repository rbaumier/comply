//! OxcCheck backend for react-no-sort-for-extrema.
//!
//! Flags `[...].sort(...)[0]` / `[...].sort(...)[arr.length - 1]` — sorting an
//! array just to pick its first or last element is O(n log n) work for an O(n)
//! result. Both the inline form (`[...].sort(...)[0]`) and an aliased form
//! (`const sorted = a.sort(...); sorted[0]`) fire.
//! `toSorted` counts as a sort.
//! `.at(0)` and `.at(-1)` count as end reads.
//!
//! The aliased form is suppressed when the sorted binding is used somewhere
//! beyond extremum extraction — passed to a function, indexed at a non-extremum
//! position (`sorted[index]`), etc. Such uses need the full sorted order (rank /
//! percentile accesses), so the `Math.min` / `Math.max` suggestion would break
//! them. A binding read only via `[0]`, `[length - 1]`, and the `.length` inside
//! such a last-index access still fires.
//! `.at(0)` and `.at(-1)` reads fire too.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{CallExpression, Expression, IdentifierReference, UnaryOperator};
use oxc_semantic::{AstNodes, NodeId};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_sort_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    matches!(member.property.name.as_str(), "sort" | "toSorted")
}

fn is_zero(expr: &Expression) -> bool {
    match expr {
        Expression::NumericLiteral(n) => n.value == 0.0 && n.raw.as_ref().is_some_and(|r| r == "0"),
        _ => false,
    }
}

/// The literal `-1`, as `.at(-1)` takes it.
fn is_minus_one(expr: &Expression) -> bool {
    let Expression::UnaryExpression(unary) = expr else {
        return false;
    };
    unary.operator == UnaryOperator::UnaryNegation
        && matches!(&unary.argument, Expression::NumericLiteral(n) if n.value == 1.0)
}

/// The array that `.at(0)` or `.at(-1)` reads.
fn at_end_object<'c, 'a>(call: &'c CallExpression<'a>) -> Option<&'c Expression<'a>> {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let [argument] = call.arguments.as_slice() else {
        return None;
    };
    let argument = argument.as_expression()?;
    let at_end =
        member.property.name.as_str() == "at" && (is_zero(argument) || is_minus_one(argument));
    at_end.then_some(&member.object)
}

fn is_length_minus_one(expr: &Expression) -> bool {
    let Expression::BinaryExpression(bin) = expr else {
        return false;
    };
    if bin.operator != oxc_ast::ast::BinaryOperator::Subtraction {
        return false;
    }
    // right must be `1`
    let Expression::NumericLiteral(right) = &bin.right else {
        return false;
    };
    if right.value != 1.0 {
        return false;
    }
    // left must be `<something>.length`
    let Expression::StaticMemberExpression(left) = &bin.left else {
        return false;
    };
    left.property.name.as_str() == "length"
}

/// Walk ancestors to find if this identifier was bound to a `.sort()` call
/// in a preceding variable declaration.
fn identifier_bound_to_sort<'a>(
    node: &oxc_semantic::AstNode<'a>,
    name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    // Walk up to find an enclosing block/program
    for ancestor in nodes.ancestors(node.id()).skip(1) {
        match ancestor.kind() {
            AstKind::FunctionBody(body) => {
                for stmt in &body.statements {
                    if let oxc_ast::ast::Statement::VariableDeclaration(decl) = stmt {
                        for declarator in &decl.declarations {
                            if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) =
                                &declarator.id
                                && id.name.as_str() == name
                                    && let Some(init) = &declarator.init {
                                        return is_sort_call(init);
                                    }
                        }
                    }
                }
                return false;
            }
            AstKind::Program(program) => {
                for stmt in &program.body {
                    if let oxc_ast::ast::Statement::VariableDeclaration(decl) = stmt {
                        for declarator in &decl.declarations {
                            if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) =
                                &declarator.id
                                && id.name.as_str() == name
                                    && let Some(init) = &declarator.init {
                                        return is_sort_call(init);
                                    }
                        }
                    }
                }
                return false;
            }
            _ => continue,
        }
    }
    false
}

/// True when the sort-bound identifier is used somewhere OTHER than extracting
/// `[0]` / `[length - 1]` (or reading `.length` for such a last-index access),
/// i.e. the full sorted order is needed (percentile / rank accesses, passing the
/// array to a function, …), so the `Math.min` / `Math.max` suggestion does not
/// apply.
///
/// Resolves `ident` to its symbol and inspects every resolved reference. A
/// reference is an *extremum use* when its parent is a `ComputedMemberExpression`
/// whose object is that reference and whose index is `[0]` or `[length - 1]`, or
/// a `StaticMemberExpression` reading `.length`. Any other parent — a call
/// argument, a non-extremum index (`sorted[index]`), a spread, a bare return — is
/// a non-extremum use and makes this return `true`.
///
/// An unresolved reference (no `reference_id` or no bound symbol) is treated as
/// "no non-extremum use found", leaving the caller's existing flagging path
/// unchanged.
fn binding_used_beyond_extrema(
    ident: &IdentifierReference,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(symbol_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let nodes = semantic.nodes();
    scoping
        .get_resolved_references(symbol_id)
        .any(|reference| !is_extremum_use(nodes, reference.node_id()))
}

/// Whether `reference` reads an end of its array.
/// That is `[0]`, `[length - 1]`, `.at(0)`, `.at(-1)` or `.length`.
/// The reference must be the member's object, never its index.
fn is_extremum_use(nodes: &AstNodes<'_>, reference: NodeId) -> bool {
    let ref_span = nodes.get_node(reference).kind().span();
    let parent = nodes.parent_id(reference);
    match nodes.kind(parent) {
        AstKind::ComputedMemberExpression(member) => {
            member.object.span() == ref_span
                && (is_zero(&member.expression) || is_length_minus_one(&member.expression))
        }
        AstKind::StaticMemberExpression(member) if member.object.span() == ref_span => {
            match member.property.name.as_str() {
                "length" => true,
                // `sorted.at(0)`: the call is the member's parent.
                "at" => matches!(
                    nodes.parent_kind(parent),
                    AstKind::CallExpression(call)
                        if at_end_object(call).is_some_and(|object| object.span() == ref_span)
                ),
                _ => false,
            }
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ComputedMemberExpression, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // The array read at one end, and the read itself.
        let (object, read_span) = match node.kind() {
            AstKind::ComputedMemberExpression(subscript) => {
                let expr = &subscript.expression;
                if !is_zero(expr) && !is_length_minus_one(expr) {
                    return;
                }
                (&subscript.object, subscript.span())
            }
            AstKind::CallExpression(call) => {
                let Some(object) = at_end_object(call) else {
                    return;
                };
                (object, call.span())
            }
            _ => return,
        };

        let direct_sort = is_sort_call(object);
        let aliased_sort = if let Expression::Identifier(ident) = object {
            if identifier_bound_to_sort(node, ident.name.as_str(), semantic) {
                // The sorted binding is also used beyond extremum extraction
                // (rank / percentile accesses, passed to a function, …): the full
                // sorted order is needed, so the `Math.min` / `Math.max`
                // suggestion does not apply — leave it alone.
                if binding_used_beyond_extrema(ident, semantic) {
                    return;
                }
                true
            } else {
                false
            }
        } else {
            false
        };

        if !direct_sort && !aliased_sort {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, read_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.sort(...)` / `.toSorted(...)` read at `[0]`, `[length-1]`, `.at(0)` or \
                      `.at(-1)` picks an extremum via O(n log n) work — \
                      use `Math.min` / `Math.max` or a single-pass fold."
                .into(),
            severity: Severity::Error,
            span: None,
        });
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

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "src/index.ts")
    }

    // ── Inline form still fires (unchanged) ────────────────────────────────

    #[test]
    fn flags_inline_sort_index_zero() {
        let diags = run_on("const min = [...a].sort((x, y) => x - y)[0];");
        assert_eq!(diags.len(), 1, "unexpected: {diags:?}");
    }

    // ── Pure min+max binding still fires (binding used only for extrema) ────

    #[test]
    fn flags_aliased_sort_used_only_for_extrema() {
        // Every reference is an extremum / `.length` use, so suppression must
        // NOT fire — the `Math.min` / `Math.max` rewrite applies.
        let src = "\
const sorted = a.sort((x, y) => x - y);
const min = sorted[0];
const max = sorted[sorted.length - 1];";
        let diags = run_on(src);
        assert_eq!(diags.len(), 2, "unexpected: {diags:?}");
    }

    #[test]
    fn flags_aliased_to_sorted_read_at_both_ends() {
        // `.at(-1)` is an end read, so it must not suppress `[0]`.
        let src = "\
const sorted = a.toSorted((x, y) => x - y);
const min = sorted[0];
const max = sorted.at(-1);";
        let diags = run_on(src);
        assert_eq!(diags.len(), 2, "unexpected: {diags:?}");
    }

    #[test]
    fn allows_aliased_to_sorted_read_in_the_middle() {
        let src = "\
const sorted = a.toSorted((x, y) => x - y);
const min = sorted.at(0);
const runnerUp = sorted.at(-2);";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    // ── Non-extremum uses suppress the aliased form (#4410) ────────────────

    #[test]
    fn allows_sorted_passed_to_a_function() {
        // vercel/ai shape: `sorted` is also passed to `pct`, so the full sorted
        // order is needed — the extremum reads are incidental.
        let src = "\
const sorted = [...a].sort((x, y) => x - y);
const r = { min: sorted[0], max: sorted[sorted.length - 1], p10: pct(sorted, 0.1) };";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }

    #[test]
    fn allows_sorted_indexed_at_non_extremum_position() {
        // es-toolkit shape: `sorted[index]` is a non-extremum access, so the sort
        // is required for its full order.
        let src = "\
function f(a, p) {
  const sorted = a.slice().sort((x, y) => x - y);
  if (p === 0) return sorted[0];
  const index = Math.ceil(sorted.length * (p / 100)) - 1;
  return sorted[index];
}";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected: {diags:?}");
    }
}
