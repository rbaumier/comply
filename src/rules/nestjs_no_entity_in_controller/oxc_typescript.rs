//! nestjs-no-entity-in-controller oxc backend — flag an ORM entity (a type
//! imported from a `*.entity` module) only when a `@Controller` route handler
//! places it in return (output) position, leaking the persistence model into
//! the HTTP layer. An entity used solely as a parameter / decorator-injected
//! input never reaches the HTTP response, so it is not flagged; neither is a
//! non-route helper method, which is not part of the HTTP layer.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Class, ClassElement, ImportDeclarationSpecifier, MethodDefinition, TSType, TSTypeName,
};
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

/// NestJS HTTP route-handler method decorators — a method carrying one of these
/// answers an HTTP request, so its return value is placed in the response.
const ROUTE_DECORATORS: &[&str] =
    &["@Get", "@Post", "@Put", "@Patch", "@Delete", "@All", "@Options", "@Head"];

fn is_nestjs_controller_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@Controller")
}

/// True when `method` carries a NestJS HTTP route decorator (`@Get`, `@Post`, …).
fn method_is_route_handler(method: &MethodDefinition, source: &str) -> bool {
    method.decorators.iter().any(|dec| {
        let text = &source[dec.span.start as usize..dec.span.end as usize];
        ROUTE_DECORATORS.iter().any(|d| text.starts_with(d))
    })
}

/// True when the module specifier is a `*.entity` file — the TypeORM/NestJS
/// convention for a persistence entity (e.g. `.../workspace.entity`). Utility
/// types imported from `typeorm` (e.g. `QueryDeepPartialEntity`) or DTO modules
/// do not match.
fn is_entity_module(source: &str) -> bool {
    let segment = source.rsplit('/').next().unwrap_or(source);
    let stem = segment
        .strip_suffix(".ts")
        .or_else(|| segment.strip_suffix(".js"))
        .unwrap_or(segment);
    stem.ends_with(".entity")
}

/// True when `ty` references one of `entities` — as a bare type (`Entity`), an
/// array (`Entity[]`), or nested inside a generic wrapper or union
/// (`Promise<Entity>`, `Promise<Entity[]>`, `Promise<Entity | null>`).
fn type_references_entity(ty: &TSType, entities: &HashSet<&str>) -> bool {
    match ty {
        TSType::TSTypeReference(type_ref) => {
            if let TSTypeName::IdentifierReference(id) = &type_ref.type_name
                && entities.contains(id.name.as_str())
            {
                return true;
            }
            type_ref.type_arguments.as_ref().is_some_and(|args| {
                args.params.iter().any(|p| type_references_entity(p, entities))
            })
        }
        TSType::TSArrayType(arr) => type_references_entity(&arr.element_type, entities),
        TSType::TSUnionType(union_type) => {
            union_type.types.iter().any(|t| type_references_entity(t, entities))
        }
        _ => false,
    }
}

/// True when `class` carries a `@Controller` decorator.
fn class_is_controller(class: &Class, source: &str) -> bool {
    class.decorators.iter().any(|dec| {
        source[dec.span.start as usize..dec.span.end as usize].starts_with("@Controller")
    })
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
        if !is_nestjs_controller_file(ctx.source) {
            return Vec::new();
        }

        // Local binding names imported from a `*.entity` module — the ORM
        // entities whose presence in a return type leaks the persistence model.
        let mut entity_names: HashSet<&str> = HashSet::new();
        for node in semantic.nodes().iter() {
            let AstKind::ImportDeclaration(import) = node.kind() else { continue };
            if !is_entity_module(import.source.value.as_str()) {
                continue;
            }
            let Some(specifiers) = &import.specifiers else { continue };
            for spec in specifiers {
                let local = match spec {
                    ImportDeclarationSpecifier::ImportSpecifier(s) => s.local.name.as_str(),
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => s.local.name.as_str(),
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => s.local.name.as_str(),
                };
                entity_names.insert(local);
            }
        }
        if entity_names.is_empty() {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let AstKind::Class(class) = node.kind() else { continue };
            if !class_is_controller(class, ctx.source) {
                continue;
            }
            for element in &class.body.body {
                let ClassElement::MethodDefinition(method) = element else { continue };
                if !method_is_route_handler(method, ctx.source) {
                    continue;
                }
                let Some(return_type) = &method.value.return_type else { continue };
                if !type_references_entity(&return_type.type_annotation, &entity_names) {
                    continue;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, return_type.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Controller method returns an ORM entity — return a DTO from the \
                              service instead of leaking the persistence model into the HTTP layer."
                        .to_string(),
                    severity: Severity::Warning,
                    span: None,
                });
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "ctrl.ts")
    }

    #[test]
    fn flags_entity_in_promise_return() {
        let src = "import { UserEntity } from './user.entity';\n\
            @Controller('x') class C { @Get() find(): Promise<UserEntity> { return this.svc.find(); } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_entity_array_return() {
        let src = "import { UserEntity } from './user.entity';\n\
            @Controller('x') class C { @Get() list(): UserEntity[] { return []; } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_entity_in_nested_promise_array_return() {
        // Promise<UserEntity[]> exercises the generic + array recursion.
        let src = "import { UserEntity } from './user.entity';\n\
            @Controller('x') class C { @Get() list(): Promise<UserEntity[]> { return x; } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_entity_in_promise_union_return() {
        // Promise<UserEntity | null> exercises the union arm of the walker.
        let src = "import { UserEntity } from './user.entity';\n\
            @Controller('x') class C { @Get() find(): Promise<UserEntity | null> { return x; } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_entity_return_on_exported_controller() {
        // The `export class` shape (as in the real controllers) is still detected.
        let src = "import { UserEntity } from './user.entity';\n\
            @Controller('x') export class C { @Get() find(): Promise<UserEntity> { return x; } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_non_route_helper_returning_entity() {
        // A method with no HTTP route decorator is not part of the HTTP layer,
        // so an entity in its return type does not leak into a response.
        let src = "import { UserEntity } from './user.entity';\n\
            @Controller('x') class C {\n\
              @Get() find(): Promise<UserDto> { return this.load(); }\n\
              private load(): Promise<UserEntity> { return this.repo.find(); }\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_aliased_entity_return() {
        // `import { UserEntity as User }` binds the entity locally as `User`;
        // the return type references the local name, so it still flags.
        let src = "import { UserEntity as User } from './user.entity';\n\
            @Controller('x') class C { @Get() find(): Promise<User> { return x; } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_entity_as_injected_input_returning_dto() {
        // Issue #7425: the entity types a decorator-injected input param and the
        // method returns a DTO — the entity never enters the HTTP response.
        let src = "import { WorkspaceEntity } from 'src/engine/core-modules/workspace/workspace.entity';\n\
            @Controller('rest/webhooks') export class WebhookController {\n\
              @Get() async findAll(@AuthWorkspace() workspace: WorkspaceEntity): Promise<WebhookDTO[]> { return []; }\n\
            }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_entity_as_body_param_void_return() {
        // Entity in a `@Body()` parameter (input), method returns nothing.
        let src = "import { UserEntity } from './user.entity';\n\
            @Controller('x') class C { @Post() create(@Body() u: UserEntity): void {} }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_typeorm_utility_type() {
        // `QueryDeepPartialEntity` is imported from `typeorm`, not a `*.entity`
        // module, so the name-suffix coincidence must not flag it.
        let src = "import { QueryDeepPartialEntity } from 'typeorm';\n\
            @Controller('x') class C { @Patch() update(@Body() b: QueryDeepPartialEntity<T>) {} }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_method_without_return_type() {
        // No return-type annotation — the AST cannot cheaply prove an entity is
        // returned, so stay FP-safe.
        let src = "import { UserEntity } from './user.entity';\n\
            @Controller('x') class C { @Get() list() { return x; } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_dto_return() {
        // A DTO return type is exactly the recommended shape.
        let src = "import { CreateUserDto } from './dto/create-user.dto';\n\
            @Controller('x') class C { @Post() create(): CreateUserDto { return x; } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn only_inspects_controller_classes() {
        // A non-`@Controller` class returning an entity is not the HTTP layer.
        let src = "import { UserEntity } from './user.entity';\n\
            @Controller('x') class Ctrl { @Get() ok(): void {} }\n\
            class Service { find(): Promise<UserEntity> { return x; } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_controller_files() {
        let src = "import { UserEntity } from './user.entity';\n\
            class Service { find(): Promise<UserEntity> { return x; } }";
        assert!(run(src).is_empty());
    }
}
