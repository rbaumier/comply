//! Flags structs with more than `MAX_FIELDS` field declarations as god objects
//! that should be decomposed into smaller types. Structs deriving a clap CLI
//! parser/args (`#[derive(... Parser ...)]` / `#[derive(... Args ...)]`) are
//! exempt: each field is a command-line flag, so the field count reflects the
//! CLI surface, not a decomposable design smell.

use crate::diagnostic::{Diagnostic, Severity};

const MAX_FIELDS: usize = 15;

/// True when the struct derives a clap CLI parser/args (`#[derive(... Parser ...)]`
/// or `#[derive(... Args ...)]`). Such a struct is a flat CLI interface: each
/// field is a command-line flag, so a large field count is idiomatic, not a
/// decomposable god object. Attributes are preceding `attribute_item` siblings.
fn derives_clap_cli(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if let Ok(text) = s.utf8_text(source)
            && text.contains("derive")
            && (text.contains("Parser") || text.contains("Args"))
        {
            return true;
        }
        sibling = s.prev_named_sibling();
    }
    false
}

crate::ast_check! { on ["struct_item"] prefilter = ["struct"] => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "field_declaration_list" { return; }
    if derives_clap_cli(node, source) { return; }

    let mut count = 0usize;
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "field_declaration" {
            count += 1;
        }
    }

    if count <= MAX_FIELDS { return; }

    let name = node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("?");

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Struct `{name}` has {count} fields (limit: {MAX_FIELDS}) — decompose into smaller types."),
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
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    fn make_struct(field_count: usize) -> String {
        let fields: String = (0..field_count)
            .map(|i| format!("    f{i}: u32,\n"))
            .collect();
        format!("struct Big {{\n{fields}}}")
    }

    #[test]
    fn allows_15_fields() {
        assert!(run(&make_struct(15)).is_empty());
    }

    #[test]
    fn flags_16_fields() {
        let diags = run(&make_struct(16));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("16 fields"));
    }

    #[test]
    fn flags_large_struct() {
        let diags = run(&make_struct(30));
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Big"));
    }

    #[test]
    fn allows_small_struct() {
        assert!(run("struct Point { x: f64, y: f64 }").is_empty());
    }

    #[test]
    fn allows_tuple_struct() {
        assert!(run("struct Wrapper(u32);").is_empty());
    }

    #[test]
    fn allows_unit_struct() {
        assert!(run("struct Unit;").is_empty());
    }

    #[test]
    fn allows_clap_parser_struct() {
        let src = format!("#[derive(clap::Parser)]\n{}", make_struct(64));
        assert!(run(&src).is_empty());
    }

    #[test]
    fn allows_clap_parser_in_derive_list() {
        let src = format!("#[derive(Debug, Clone, Parser)]\n{}", make_struct(30));
        assert!(run(&src).is_empty());
    }

    #[test]
    fn allows_clap_args_struct() {
        let src = format!("#[derive(Args)]\n{}", make_struct(30));
        assert!(run(&src).is_empty());
    }

    #[test]
    fn flags_large_struct_with_non_clap_derive() {
        let src = format!("#[derive(Debug, Clone)]\n{}", make_struct(30));
        let diags = run(&src);
        assert_eq!(diags.len(), 1);
    }
}
