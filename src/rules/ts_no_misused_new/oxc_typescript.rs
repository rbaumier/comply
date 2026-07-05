//! OxcCheck backend for ts-no-misused-new.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, TSSignature, TSType, TSTypeAnnotation, TSTypeName};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class, AstType::TSInterfaceDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::Class(class) => {
                // Flag `new()` method in class body
                for element in &class.body.body {
                    let ClassElement::MethodDefinition(method) = element else { continue };
                    let name = method.key.static_name();
                    if name.as_deref() != Some("new") {
                        continue;
                    }
                    // The "you meant `constructor`" misuse: a `new` method
                    // whose declared return type is the containing class type
                    // (or `this`). A `new` method returning any other type —
                    // e.g. a generic type parameter — is a valid factory.
                    let class_name = class.id.as_ref().map(|id| id.name.as_str());
                    if !returns_containing_class(method.value.return_type.as_deref(), class_name) {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, method.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Class cannot have method named `new` — use `constructor` instead."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            AstKind::TSInterfaceDeclaration(iface) => {
                // Flag `constructor()` method in interface body
                for sig in &iface.body.body {
                    let TSSignature::TSMethodSignature(method) = sig else { continue };
                    let name = method.key.static_name();
                    if name.as_deref() != Some("constructor") {
                        continue;
                    }
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, method.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message:
                            "Interfaces cannot be constructed — use `new(): Type` instead of `constructor()`."
                                .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}

/// Whether a class method's declared return type denotes the containing class
/// type — its bare name (a `TSTypeReference` whose identifier equals the class
/// id) or `this`. This is the misused-`new` signal (the method should be a
/// `constructor`). An absent or unrelated return type — including a generic
/// type parameter or a namespaced (`ns.Foo`) type, neither of which can name
/// the containing class from within its own scope — is a valid factory, not a
/// misuse. Follows the same structural signal as typescript-eslint's
/// `isMatchingParentType`, additionally treating `this` as the instance type.
fn returns_containing_class(
    return_type: Option<&TSTypeAnnotation>,
    class_name: Option<&str>,
) -> bool {
    let Some(return_type) = return_type else { return false };
    match &return_type.type_annotation {
        TSType::TSThisType(_) => true,
        TSType::TSTypeReference(reference) => {
            let Some(class_name) = class_name else { return false };
            match &reference.type_name {
                TSTypeName::IdentifierReference(id) => id.name.as_str() == class_name,
                // A namespaced (`ns.Foo`) or `this`-qualified name never refers
                // to the containing class, which is in scope under its bare id.
                TSTypeName::QualifiedName(_) | TSTypeName::ThisExpression(_) => false,
            }
        }
        _ => false,
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

    #[test]
    fn no_fp_on_factory_returning_type_parameter() {
        // Issue #7299: mikro-orm EntitySchema — `new(...)` is a factory whose
        // return type is the generic type parameter `Entity`, not the class.
        let src = "export class EntitySchema<Entity = any, Class extends EntityClass<Entity> = EntityClass<Entity>> { new(...params: ConstructorParameters<Class>): Entity { return new (this._meta.class as any)(...params); } }";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "src/metadata/EntitySchema.ts");
        assert!(d.is_empty(), "factory `new` returning a type parameter must not be flagged: {d:?}");
    }

    #[test]
    fn no_fp_on_factory_returning_other_named_type() {
        let src = "class Foo { new(): Bar { return new Bar(); } }";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "src/foo.ts");
        assert!(d.is_empty(), "factory `new` returning a different named type must not be flagged: {d:?}");
    }

    #[test]
    fn no_fp_on_factory_returning_namespaced_type() {
        // A namespaced return type cannot name the containing class from its
        // own scope, so `ns.Foo` is a different type — a valid factory.
        let src = "class Foo { new(): ns.Foo; }";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "src/foo.ts");
        assert!(d.is_empty(), "factory `new` returning a namespaced type must not be flagged: {d:?}");
    }

    #[test]
    fn no_fp_on_new_without_return_annotation() {
        let src = "class Foo { new() { return new Foo(); } }";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "src/foo.ts");
        assert!(d.is_empty(), "`new` without a return annotation must not be flagged: {d:?}");
    }

    #[test]
    fn flags_new_declaration_returning_containing_class() {
        let src = "class Foo { new(): Foo; }";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "src/foo.ts");
        assert_eq!(d.len(), 1, "`new(): Foo` must be flagged: {d:?}");
    }

    #[test]
    fn flags_new_body_returning_containing_class() {
        let src = "class Foo { new(): Foo { return this; } }";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "src/foo.ts");
        assert_eq!(d.len(), 1, "`new(): Foo` with a body must be flagged: {d:?}");
    }

    #[test]
    fn flags_new_returning_this() {
        let src = "class Foo { new(): this; }";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "src/foo.ts");
        assert_eq!(d.len(), 1, "`new(): this` must be flagged: {d:?}");
    }

    #[test]
    fn still_flags_interface_constructor() {
        let src = "interface Foo { constructor(): void; }";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "src/foo.ts");
        assert_eq!(d.len(), 1, "interface `constructor()` must still be flagged: {d:?}");
    }
}
