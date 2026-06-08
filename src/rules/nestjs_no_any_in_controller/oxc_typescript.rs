//! nestjs-no-any-in-controller oxc backend — flag controller parameters
//! decorated with `@Body()`/`@Query()`/`@Param()`/`@Headers()` typed as `any`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use std::sync::Arc;

const PARAM_DECORATORS: &[&str] = &["Body", "Query", "Param", "Headers"];

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/") || crate::oxc_helpers::source_contains(source, "@Controller")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_nestjs_file(ctx.source) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::FormalParameter(param) = node.kind() else { continue };

            // Check if parameter has a relevant NestJS decorator.
            let deco_text = param.decorators.iter().find_map(|d| {
                let text = &ctx.source[d.span.start as usize..d.span.end as usize];
                // text looks like `@Body()` or `@Query('key')` etc.
                if PARAM_DECORATORS.iter().any(|name| text.starts_with(&format!("@{name}"))) {
                    Some(text.to_string())
                } else {
                    None
                }
            });
            let Some(deco) = deco_text else { continue };

            // Check if the type annotation is `any`.
            let Some(ref type_ann) = param.pattern.type_annotation else { continue };
            let is_any = matches!(&type_ann.type_annotation, TSType::TSAnyKeyword(_));
            if !is_any {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, param.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("`{deco}` parameter typed as `any` bypasses NestJS validation pipeline — use a DTO."),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_body_any() {
        let src = "import { Controller, Post, Body } from '@nestjs/common';\n@Controller() class C { @Post() create(@Body() body: any) { return body; } }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_body_with_dto() {
        let src = "import { Controller, Post, Body } from '@nestjs/common';\n@Controller() class C { @Post() create(@Body() body: CreateUserDto) { return body; } }";
        assert!(run(src).is_empty());
    }
}
