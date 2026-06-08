//! Flag classes named `*Dto` whose fields have no class-validator decorator.

use crate::diagnostic::{Diagnostic, Severity};

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/") || crate::oxc_helpers::source_contains(source, "class-validator")
}

fn property_has_validator(prop: tree_sitter::Node, source: &[u8]) -> bool {
    // Decorators are direct children of the property definition.
    let mut cursor = prop.walk();
    for child in prop.children(&mut cursor) {
        if child.kind() == "decorator" {
            let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
            if text.starts_with("@Is") || text.starts_with("@Min") || text.starts_with("@Max")
                || text.starts_with("@Length") || text.starts_with("@Matches")
                || text.starts_with("@Allow") || text.starts_with("@ValidateNested")
                || text.starts_with("@Type") {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { on ["class_declaration"] => |node, source, ctx, diagnostics|
    if !is_nestjs_file(ctx.source) { return; }
    let Some(name_node) = node.child_by_field_name("name") else { return; };
    let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("");
    if !name.ends_with("Dto") { return; }
    let Some(body) = node.child_by_field_name("body") else { return; };
    let mut cursor = body.walk();
    let mut total_props = 0usize;
    let mut undecorated = 0usize;
    for child in body.children(&mut cursor) {
        if child.kind() == "public_field_definition" || child.kind() == "property_definition" {
            total_props += 1;
            if !property_has_validator(child, source) { undecorated += 1; }
        }
    }
    if total_props == 0 { return; }
    if undecorated == total_props {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &name_node,
            super::META.id,
            format!("DTO `{name}` has no class-validator decorators — request bodies will not be validated."),
            Severity::Warning,
        ));
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
