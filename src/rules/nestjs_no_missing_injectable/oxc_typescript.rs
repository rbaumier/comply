//! nestjs-no-missing-injectable oxc backend.
//!
//! A `*Service` / `*Repository` / `*UseCase` / `*Provider` class is flagged as
//! a missing provider unless it carries a DI marker (`@Injectable`) or a
//! GraphQL/ORM data-type decorator (`@ObjectType` / `@InputType` / `@ArgsType`
//! / `@Scalar` / `@Entity`). A data class is mutually exclusive with being a DI
//! provider, so those decorators exempt the class regardless of its name suffix.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/")
}

/// A class carrying any of these decorators is not a missing-provider: either it
/// already declares DI intent (`@Injectable`) or it is a GraphQL/ORM data type
/// (`@ObjectType` / `@InputType` / `@ArgsType` / `@Scalar` / `@Entity`), which
/// is mutually exclusive with being a DI provider.
fn is_data_or_injectable_decorator(dec_text: &str) -> bool {
    dec_text.contains("@Injectable")
        || dec_text.contains("@ObjectType")
        || dec_text.contains("@InputType")
        || dec_text.contains("@ArgsType")
        || dec_text.contains("@Scalar")
        || dec_text.contains("@Entity")
}

fn class_name_looks_like_provider(name: &str) -> bool {
    name.ends_with("Service")
        || name.ends_with("Repository")
        || name.ends_with("UseCase")
        || name.ends_with("Provider")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@nestjs/"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Class(class) = node.kind() else {
            return;
        };
        if !is_nestjs_file(ctx.source) {
            return;
        }

        let Some(id) = &class.id else { return };
        let name = id.name.as_str();
        if !class_name_looks_like_provider(name) {
            return;
        }

        // A class that already carries a DI marker (`@Injectable`) or a
        // GraphQL/ORM data-type decorator is not a missing-provider regardless
        // of its name suffix.
        for decorator in &class.decorators {
            let dec_start = decorator.span.start as usize;
            let dec_end = decorator.span.end as usize;
            let dec_text = &ctx.source[dec_start..dec_end];
            if is_data_or_injectable_decorator(dec_text) {
                return;
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, id.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Class `{name}` looks like a NestJS provider but is missing `@Injectable()`."
            ),
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
    ) -> Vec<Diagnostic> {
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
    fn flags_bare_service_class() {
        // Load-bearing guard: a `*Service` class with no decorator in a
        // `@nestjs/` file is still a missing provider.
        let src = "import { Module } from '@nestjs/common';\n\
                   export class FooService {\n  doWork() {}\n}\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_injectable_service_class() {
        let src = "import { Injectable } from '@nestjs/common';\n\
                   @Injectable()\nexport class BarService {\n  doWork() {}\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_object_type_repository() {
        // Issue #3725: a GraphQL `@ObjectType` data class named `*Repository`.
        let src = "import { Field, ObjectType } from '@nestjs/graphql';\n\
                   @ObjectType()\nexport class GitRepository {\n  @Field(() => String) id!: string;\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_input_type_service() {
        // Issue #3725: a GraphQL `@InputType` data class named `*Service`.
        let src = "import { Field, InputType } from '@nestjs/graphql';\n\
                   @InputType()\nexport class RedesignNewService {\n  @Field(() => String) name!: string;\n}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_entity_repository() {
        // A TypeORM `@Entity` data class named `*Repository`.
        let src = "import { Module } from '@nestjs/common';\n\
                   import { Entity, Column } from 'typeorm';\n\
                   @Entity()\nexport class FooRepository {\n  @Column() id!: string;\n}\n";
        assert!(run(src).is_empty());
    }
}
