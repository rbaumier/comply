//! no-delete oxc backend — flag the `delete` operator.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::UnaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::UnaryExpression(unary) = node.kind() else {
            return;
        };
        if unary.operator != oxc_ast::ast::UnaryOperator::Delete {
            return;
        }
        // Test files delete `process.env` keys and fixture properties in
        // teardown — bounded to the test scope with no non-mutating equivalent.
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        // Converting a PropertyDescriptor from a data descriptor to an accessor
        // descriptor requires deleting `value`/`writable` before assigning
        // `get`/`set` — ECMAScript forbids a descriptor from carrying both. This
        // `delete` is on a freshly-obtained local descriptor, not the foot-gun
        // the rule targets.
        if is_descriptor_data_key_delete(&unary.argument, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, unary.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`delete` mutates the target object — return a new object without the property instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when `arg` is `desc.value` / `desc.writable` where `desc` is a
/// `PropertyDescriptor`-typed binding — the data-descriptor keys that must be
/// deleted to convert a data descriptor to an accessor descriptor.
fn is_descriptor_data_key_delete(
    arg: &oxc_ast::ast::Expression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::ast::Expression;
    let Expression::StaticMemberExpression(member) = arg else {
        return false;
    };
    if !matches!(member.property.name.as_str(), "value" | "writable") {
        return false;
    }
    let Expression::Identifier(object) = &member.object else {
        return false;
    };
    binding_is_property_descriptor(object, semantic)
}

/// Resolve an identifier reference to its declaration and decide whether that
/// binding holds a `PropertyDescriptor` — a declarator typed `PropertyDescriptor`
/// (optionally `| undefined`), or initialised from
/// `Object.getOwnPropertyDescriptor(...)` / `Reflect.getOwnPropertyDescriptor(...)`.
fn binding_is_property_descriptor(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind;
    let scoping = semantic.scoping();
    let Some(symbol_id) = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
    else {
        return false;
    };
    let decl_id = scoping.symbol_declaration(symbol_id);
    match semantic.nodes().kind(decl_id) {
        AstKind::VariableDeclarator(decl) => {
            if let Some(type_ann) = &decl.type_annotation
                && type_is_property_descriptor(&type_ann.type_annotation)
            {
                return true;
            }
            decl.init.as_ref().is_some_and(initializer_is_get_descriptor)
        }
        AstKind::FormalParameter(param) => param
            .type_annotation
            .as_ref()
            .is_some_and(|ann| type_is_property_descriptor(&ann.type_annotation)),
        _ => false,
    }
}

/// Whether a type annotation denotes a `PropertyDescriptor`, including the
/// `PropertyDescriptor | undefined` union returned by `getOwnPropertyDescriptor`.
fn type_is_property_descriptor(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    match ty {
        TSType::TSTypeReference(tref) => matches!(
            &tref.type_name,
            TSTypeName::IdentifierReference(id) if id.name.as_str() == "PropertyDescriptor"
        ),
        TSType::TSUnionType(union) => union.types.iter().any(type_is_property_descriptor),
        _ => false,
    }
}

/// Whether an initializer is `Object.getOwnPropertyDescriptor(...)` or
/// `Reflect.getOwnPropertyDescriptor(...)`.
fn initializer_is_get_descriptor(expr: &oxc_ast::ast::Expression) -> bool {
    use oxc_ast::ast::Expression;
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name.as_str() != "getOwnPropertyDescriptor" {
        return false;
    }
    matches!(
        &member.object,
        Expression::Identifier(id) if matches!(id.name.as_str(), "Object" | "Reflect")
    )
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
mod oxc_tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_in_test_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &file)
    }

    #[test]
    fn flags_delete_operator() {
        assert_eq!(run("delete obj.prop;").len(), 1);
    }

    #[test]
    fn skips_in_test_file_issue_582() {
        // Test teardown deletes `process.env` keys; bounded to test scope.
        assert!(run_in_test_file(r#"delete process.env["API_SENTRY_DSN"];"#).is_empty());
    }

    #[test]
    fn skips_descriptor_data_to_accessor_conversion_issue_5494() {
        // Converting a data descriptor to an accessor descriptor must delete
        // `value`/`writable` before assigning `get` (solidjs/solid store proxy).
        let src = r#"
            function proxyDescriptor(target, property) {
              const desc = Reflect.getOwnPropertyDescriptor(target, property);
              if (!desc || desc.get) return desc;
              delete desc.value;
              delete desc.writable;
              desc.get = () => target[property];
              return desc;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_object_get_own_property_descriptor_binding() {
        let src = r#"
            function f(o, k) {
              const d = Object.getOwnPropertyDescriptor(o, k);
              delete d.writable;
              return d;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_property_descriptor_typed_binding() {
        let src = r#"
            function f(desc: PropertyDescriptor) {
              delete desc.value;
              return desc;
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_delete_value_on_non_descriptor_binding() {
        // `value`/`writable` keys do not exempt an ordinary object.
        let src = r#"
            function f() {
              const obj = { value: 1, writable: 2 };
              delete obj.value;
              return obj;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_delete_unrelated_key_on_descriptor() {
        // Only the data-descriptor keys are exempt, not arbitrary deletes.
        let src = r#"
            function f(o, k) {
              const desc = Reflect.getOwnPropertyDescriptor(o, k);
              delete desc.configurable;
              return desc;
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
