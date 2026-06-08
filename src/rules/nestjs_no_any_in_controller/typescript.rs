//! Flag controller-method parameters decorated with `@Body()`/`@Query()`/
//! `@Param()` whose type annotation is bare `any`.

use crate::diagnostic::{Diagnostic, Severity};

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/") || crate::oxc_helpers::source_contains(source, "@Controller")
}

const PARAM_DECORATORS: &[&str] = &["@Body", "@Query", "@Param", "@Headers"];

fn parameter_decorator_text<'a>(param: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<String> {
    // Decorators on parameters are direct children inside the parameter node.
    let mut cursor = param.walk();
    for child in param.children(&mut cursor) {
        if child.kind() == "decorator" {
            let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
            if PARAM_DECORATORS.iter().any(|d| text.starts_with(d)) {
                return Some(text.to_string());
            }
        }
    }
    None
}

fn type_is_any(param: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(type_ann) = param.child_by_field_name("type") else { return false };
    let mut cursor = type_ann.walk();
    for child in type_ann.children(&mut cursor) {
        if child.kind() == "any" { return true; }
        let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
        if text.trim() == "any" { return true; }
    }
    false
}

crate::ast_check! { on ["required_parameter", "optional_parameter"] => |node, source, ctx, diagnostics|
    if !is_nestjs_file(ctx.source) { return; }
    let Some(deco) = parameter_decorator_text(node, source) else { return; };
    if !type_is_any(node, source) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("`{deco}` parameter typed as `any` bypasses NestJS validation pipeline — use a DTO."),
        Severity::Warning,
    ));
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
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
