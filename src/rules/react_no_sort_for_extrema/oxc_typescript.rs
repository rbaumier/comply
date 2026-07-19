//! OxcCheck backend for react-no-sort-for-extrema.
//!
//! Flags `[...].sort(...)[0]` / `[...].sort(...)[arr.length - 1]` — sorting an
//! array just to pick its first or last element is O(n log n) work for an O(n)
//! result. Both the inline form (`[...].sort(...)[0]`) and an aliased form
//! (`const sorted = a.sort(...); sorted[0]`) fire.
//!
//! The aliased form is suppressed when the sorted binding is used somewhere
//! beyond extremum extraction — passed to a function, indexed at a non-extremum
//! position (`sorted[index]`), etc. Such uses need the full sorted order (rank /
//! percentile accesses), so the `Math.min` / `Math.max` suggestion would break
//! them. A binding read only via `[0]`, `[length - 1]`, and the `.length` inside
//! such a last-index access still fires.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, IdentifierReference};
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
    member.property.name.as_str() == "sort"
}

fn is_zero(expr: &Expression) -> bool {
    match expr {
        Expression::NumericLiteral(n) => n.value == 0.0 && n.raw.as_ref().is_some_and(|r| r == "0"),
        _ => false,
    }
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
    scoping.get_resolved_references(symbol_id).any(|reference| {
        let ref_span = nodes.get_node(reference.node_id()).kind().span();
        let parent_kind = nodes.kind(nodes.parent_id(reference.node_id()));
        !is_extremum_use(parent_kind, ref_span)
    })
}

/// True when a reference (spanning `ref_span`) feeding `parent` extracts an
/// extremum: it is the object of a `[0]` / `[length - 1]` index, or the object of
/// a `.length` read. The `ref_span` check pins the reference to the member
/// *object* so the index of `sorted[index]` is never mistaken for an extremum
/// use.
fn is_extremum_use(parent: AstKind<'_>, ref_span: oxc_span::Span) -> bool {
    match parent {
        AstKind::ComputedMemberExpression(member) => {
            member.object.span() == ref_span
                && (is_zero(&member.expression) || is_length_minus_one(&member.expression))
        }
        AstKind::StaticMemberExpression(member) => {
            member.object.span() == ref_span && member.property.name.as_str() == "length"
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ComputedMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ComputedMemberExpression(subscript) = node.kind() else {
            return;
        };
        let expr = &subscript.expression;
        if !is_zero(expr) && !is_length_minus_one(expr) {
            return;
        }

        let direct_sort = is_sort_call(&subscript.object);
        let aliased_sort = if let Expression::Identifier(ident) = &subscript.object {
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

        let (line, column) = byte_offset_to_line_col(ctx.source, subscript.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`.sort(...)[0]` / `.sort(...)[length-1]` picks an extremum via O(n log n) work — \
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
