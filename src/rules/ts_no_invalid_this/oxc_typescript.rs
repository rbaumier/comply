//! ts-no-invalid-this OXC backend — flag `this` expressions outside
//! classes/object methods.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{AssignmentTarget, Expression};
use std::sync::Arc;

pub struct Check;

/// True when `expr` is a `*.prototype` member access (e.g. `Foo.prototype`),
/// or an identifier bound to such an access (e.g. `var proto = Foo.prototype`).
/// These are the receivers of the prototype-patching idiom, where a function
/// assigned to one of their members gains the instance as `this` at call time.
fn is_prototype_object(
    expr: &Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    match expr {
        Expression::StaticMemberExpression(member) => member.property.name == "prototype",
        Expression::Identifier(ident) => {
            let Some(ref_id) = ident.reference_id.get() else {
                return false;
            };
            let scoping = semantic.scoping();
            let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
                return false;
            };
            let decl = scoping.symbol_declaration(sym_id);
            let AstKind::VariableDeclarator(declarator) =
                semantic.nodes().kind(decl)
            else {
                return false;
            };
            matches!(
                &declarator.init,
                Some(Expression::StaticMemberExpression(member))
                    if member.property.name == "prototype"
            )
        }
        _ => false,
    }
}

/// True when `func_id` is a function expression assigned to a member of a
/// prototype object (`proto[m] = function() {}` / `Foo.prototype.m = function() {}`).
/// In that idiom `this` is bound to the instance at call time, so `this` inside
/// the function body is valid.
fn is_prototype_method_assignment(
    func_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let AstKind::AssignmentExpression(assign) = nodes.kind(nodes.parent_id(func_id)) else {
        return false;
    };
    let object = match &assign.left {
        AssignmentTarget::StaticMemberExpression(member) => &member.object,
        AssignmentTarget::ComputedMemberExpression(member) => &member.object,
        _ => return false,
    };
    is_prototype_object(object, semantic)
}

fn is_valid_this_context(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    // Walk up from the ThisExpression. The first `this`-binding boundary
    // determines validity:
    // - ArrowFunction: transparent, keep going.
    // - Function inside a MethodDefinition (class method): valid.
    // - Function that is an object-literal shorthand method: valid.
    // - Standalone Function: invalid — stop.
    // - Class: valid (property initializer, etc.).
    let mut hit_function = false;
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Class(_) => return true,
            AstKind::ArrowFunctionExpression(_) => continue,
            AstKind::Function(_) => {
                // Prototype-patching idiom: a function assigned to a member
                // of a `*.prototype` object is a method — `this` is the
                // instance at call time, so it's valid.
                if is_prototype_method_assignment(ancestor.id(), semantic) {
                    return true;
                }
                // Mark that we've entered a function scope; need to
                // check if it's wrapped in a MethodDefinition.
                hit_function = true;
            }
            AstKind::MethodDefinition(_) if hit_function => {
                // The Function was a class method — `this` is valid.
                return true;
            }
            AstKind::PropertyDefinition(_) if hit_function => {
                // Property initializer context — valid.
                return true;
            }
            AstKind::ObjectProperty(prop) if hit_function && prop.method => {
                // Object-literal shorthand method (`{ foo() { this } }`,
                // including `[Symbol.asyncIterator]() { return this; }`) —
                // `this` is bound to the object. A function-valued property
                // (`{ foo: function() {} }`) has `method == false` and stays
                // flagged.
                return true;
            }
            _ => {
                // If we already hit a standalone function (not a method),
                // any other ancestor means `this` is unbound.
                if hit_function {
                    return false;
                }
            }
        }
    }
    false
}

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

        for node in semantic.nodes().iter() {
            let AstKind::ThisExpression(this_expr) = node.kind() else {
                continue;
            };

            if is_valid_this_context(node, semantic) {
                continue;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, this_expr.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`this` used outside a class or valid context — likely a bug."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_this_at_top_level() {
        let diags = run_on("console.log(this);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_class_method() {
        assert!(run_on("class Foo { bar() { return this.x; } }").is_empty());
    }

    #[test]
    fn flags_this_in_standalone_function() {
        let diags = run_on("function foo() { return this; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_object_literal_async_iterator_method() {
        let src = "const asyncIterable = {\n  next() { return iter.next(); },\n  [Symbol.asyncIterator]() {\n    return this;\n  },\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_function_valued_property() {
        let diags = run_on("const obj = { foo: function() { return this; } };");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_this_in_prototype_patch_via_alias() {
        // Regression for #2031: `proto[method] = function() { this }` where
        // `proto` is an alias of `SomeClass.prototype`.
        let src = "var proto = SvelteDate.prototype;\nproto[method] = function (...args) {\n  return this.x.apply(this, args);\n};";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_prototype_patch_static() {
        let src = "Foo.prototype.m = function () { return this.x; };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_this_in_prototype_patch_computed() {
        let src = "Foo.prototype[k] = function () { return this.x; };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_this_in_non_prototype_member_assignment() {
        // A function assigned to a plain (non-prototype) object member is still
        // a standalone function — `this` is unbound and must fire.
        let diags = run_on("obj.m = function () { return this.x; };");
        assert_eq!(diags.len(), 1);
    }
}
