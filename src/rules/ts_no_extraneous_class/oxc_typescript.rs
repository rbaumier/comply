//! ts-no-extraneous-class OxcCheck backend — flag classes that are empty,
//! contain only a constructor, or contain only static members.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, MethodDefinitionKind};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else { return };

        // Skip classes that extend a superclass.
        if class.super_class.is_some() {
            return;
        }

        // Skip decorated classes.
        if !class.decorators.is_empty() {
            return;
        }
        // Also check parent for decorator (export @Decorator class Foo {}).
        if let Some(parent) = semantic.nodes().ancestors(node.id()).nth(1)
            && let AstKind::ExportDefaultDeclaration(_) | AstKind::ExportNamedDeclaration(_) =
                parent.kind()
            {
                // Check the source text before class for `@`.
                let class_start = class.span.start as usize;
                if class_start > 0 {
                    let before = ctx.source[..class_start].trim_end();
                    if before.ends_with(')') || before.ends_with('}') {
                        // Possible decorator — check more carefully.
                        let last_line = before.lines().last().unwrap_or("");
                        if last_line.trim_start().starts_with('@') {
                            return;
                        }
                    }
                    if before.lines().last().is_some_and(|l| l.trim_start().starts_with('@')) {
                        return;
                    }
                }
            }

        let body = &class.body;
        let members: Vec<_> = body
            .body
            .iter()
            .filter(|m| !matches!(m, ClassElement::StaticBlock(_)))
            .collect();

        if members.is_empty() {
            // In NestJS, empty exported classes are idiomatic DI tokens / DTOs /
            // entities: `@Body() dto: CreateDogDto`, `class Dog {}`. NestJS's
            // ValidationPipe requires a runtime class (not an interface) to
            // instantiate, so an empty class is "no fields yet", not dead code.
            if ctx.project.has_framework("nestjs") {
                return;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, class.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Unexpected empty class.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        let mut only_constructor = true;
        let mut only_static = true;

        for member in &members {
            match member {
                ClassElement::MethodDefinition(method) => {
                    if method.kind == MethodDefinitionKind::Constructor {
                        // Check for parameter properties.
                        for param in &method.value.params.items {
                            if param.accessibility.is_some() {
                                only_constructor = false;
                                only_static = false;
                            }
                        }
                    } else {
                        only_constructor = false;
                        if !method.r#static {
                            only_static = false;
                        }
                    }
                }
                ClassElement::PropertyDefinition(prop) => {
                    only_constructor = false;
                    if !prop.r#static {
                        only_static = false;
                    }
                }
                ClassElement::AccessorProperty(prop) => {
                    only_constructor = false;
                    if !prop.r#static {
                        only_static = false;
                    }
                }
                ClassElement::TSIndexSignature(_) => {
                    only_constructor = false;
                    only_static = false;
                }
                ClassElement::StaticBlock(_) => {}
            }
            if !only_constructor && !only_static {
                break;
            }
        }

        let msg = if only_constructor {
            "Unexpected class with only a constructor."
        } else if only_static {
            "Unexpected class with only static properties."
        } else {
            return;
        };
        let (line, column) =
            byte_offset_to_line_col(ctx.source, class.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: msg.into(),
            severity: Severity::Warning,
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

    fn run_in_nestjs(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            src,
            path,
            &crate::project::ProjectCtx::for_test_with_framework("nestjs"),
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    // Regression for #1241: NestJS DTO/entity classes start empty because they
    // are DI tokens / ValidationPipe targets, not dead namespaces.
    #[test]
    fn allows_empty_dto_class_in_nestjs_project() {
        assert!(
            run_in_nestjs("export class CreateDogDto {}", "src/dogs/dto/create-dog.dto.ts")
                .is_empty(),
            "empty DTO in a NestJS project must not be flagged"
        );
    }

    #[test]
    fn allows_empty_entity_class_in_nestjs_project() {
        assert!(
            run_in_nestjs("export class Dog {}", "src/dogs/entities/dog.entity.ts").is_empty(),
            "empty entity in a NestJS project must not be flagged"
        );
    }

    // Negative-space guard: a genuinely-extraneous empty class in a project with
    // no class-DI framework is still flagged — the exemption is framework-gated,
    // not a blanket disable.
    #[test]
    fn still_flags_empty_class_in_non_framework_project() {
        let diags = crate::rules::test_helpers::run_rule(&Check, "export class Empty {}", "src/widget.ts");
        assert_eq!(diags.len(), 1, "empty class outside a DI framework must still fire");
        assert!(diags[0].message.contains("empty"));
    }

    // The NestJS gate is scoped to the empty-class case; a static-only namespace
    // class (no constructor, never instantiated) is extraneous everywhere and
    // must still fire even inside a NestJS project.
    #[test]
    fn still_flags_static_only_class_in_nestjs_project() {
        let diags = run_in_nestjs("export class Utils { static foo() {} }", "src/utils.ts");
        assert_eq!(diags.len(), 1, "static-only namespace class must still fire in NestJS");
        assert!(diags[0].message.contains("static"));
    }
}
