//! Flags `as_expression` nodes targeting a type whose name contains
//! `Brand` or ends with `Id`/`Uuid`/`Token` (common brand suffixes),
//! except inside a function whose name contains `parse`/`make`/`create`/
//! `brand`/`to`/`from` (the canonical validator/constructor conventions).

use crate::diagnostic::{Diagnostic, Severity};

fn is_branded_name(name: &str) -> bool {
    name.contains("Brand")
        || name.ends_with("Id")
        || name.ends_with("Uuid")
        || name.ends_with("UUID")
        || name.ends_with("Token")
        || name.ends_with("Hash")
}

fn enclosing_function_name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut probe = node.parent();
    while let Some(p) = probe {
        let kind = p.kind();
        if (kind == "function_declaration" || kind == "method_definition")
            && let Some(name) = p.child_by_field_name("name")
        {
            return std::str::from_utf8(&source[name.byte_range()])
                .ok()
                .map(str::to_string);
        }
        if kind == "variable_declarator"
            && let Some(name) = p.child_by_field_name("name")
        {
            return std::str::from_utf8(&source[name.byte_range()])
                .ok()
                .map(str::to_string);
        }
        probe = p.parent();
    }
    None
}

fn is_validator_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.starts_with("parse")
        || lower.starts_with("make")
        || lower.starts_with("create")
        || lower.starts_with("brand")
        || lower.starts_with("to")
        || lower.starts_with("from")
        || lower.starts_with("as")
        || lower.contains("validate")
}

crate::ast_check! { on ["as_expression"] => |node, source, ctx, diagnostics|
    let Some(target) = node.named_child(1) else { return };
    let target_text = std::str::from_utf8(&source[target.byte_range()]).unwrap_or("");
    // Strip generic arguments: Foo<Bar> -> Foo
    let base_name = target_text.split('<').next().unwrap_or(target_text).trim();
    if !is_branded_name(base_name) {
        return;
    }

    if let Some(fn_name) = enclosing_function_name(node, source)
        && is_validator_name(&fn_name)
    {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Direct cast to branded type `{base_name}`; route through a validator/constructor function."),
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
    fn flags_direct_cast_to_brand_type() {
        let src = "function consume() { const id = 'abc' as UserId; }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_direct_cast_to_brand_suffixed() {
        let src = "function fetch() { const t = raw as AuthToken; }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_cast_inside_validator() {
        let src = "function parseUserId(x: string): UserId { return x as UserId; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_cast_to_plain_type() {
        let src = "function f() { const s = x as string; }";
        assert!(run(src).is_empty());
    }
}
