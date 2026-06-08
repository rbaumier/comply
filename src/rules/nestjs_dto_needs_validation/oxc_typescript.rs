//! nestjs-dto-needs-validation — oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::ClassElement;
use std::sync::Arc;

pub struct Check;

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/") || crate::oxc_helpers::source_contains(source, "class-validator")
}

fn property_has_validator(prop: &oxc_ast::ast::PropertyDefinition, source: &str) -> bool {
    for dec in &prop.decorators {
        let dec_text = &source[dec.span.start as usize..dec.span.end as usize];
        if dec_text.starts_with("@Is")
            || dec_text.starts_with("@Min")
            || dec_text.starts_with("@Max")
            || dec_text.starts_with("@Length")
            || dec_text.starts_with("@Matches")
            || dec_text.starts_with("@Allow")
            || dec_text.starts_with("@ValidateNested")
            || dec_text.starts_with("@Type")
        {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_nestjs_file(ctx.source) {
            return;
        }

        let AstKind::Class(class) = node.kind() else { return };

        let Some(id) = &class.id else { return };
        let name = id.name.as_str();
        if !name.ends_with("Dto") {
            return;
        }

        let mut total_props = 0usize;
        let mut undecorated = 0usize;
        for element in &class.body.body {
            let ClassElement::PropertyDefinition(prop) = element else { continue };
            total_props += 1;
            if !property_has_validator(prop, ctx.source) {
                undecorated += 1;
            }
        }

        if total_props == 0 {
            return;
        }
        if undecorated == total_props {
            let (line, column) = byte_offset_to_line_col(ctx.source, id.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "DTO `{name}` has no class-validator decorators — request bodies will not be validated."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_dto_without_validators() {
        let src = "import { Module } from '@nestjs/common';\nexport class CreateUserDto { name: string; email: string; }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_dto_with_validators() {
        let src = "import { IsString } from 'class-validator';\nexport class CreateUserDto { @IsString() name: string; @IsString() email: string; }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_non_dto_class() {
        let src = "import { Module } from '@nestjs/common';\nexport class UserService {}";
        assert!(run(src).is_empty());
    }
}
