//! Flags structs with more than `MAX_FIELDS` field declarations as god objects
//! that should be decomposed into smaller types. The god-object antipattern is
//! concentrated behavior, so the field count only matters when the struct also
//! carries methods. Two kinds of field-heavy data carriers are exempt:
//! - structs deriving a clap CLI parser/args (`#[derive(... Parser ...)]` /
//!   `#[derive(... Args ...)]`), where each field is a command-line flag; and
//! - structs with no inherent methods (no `impl Foo { fn ... }`), i.e. pure
//!   data records / serde-style DTOs whose field count mirrors an external
//!   schema rather than poor design.

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

/// True when the file declares an inherent `impl <type_name> { ... }` block (no
/// trait) containing at least one method (`function_item`). A field-heavy struct
/// with concentrated behavior is the genuine god object; one with none is a pure
/// data record (a serde-style DTO / options bag) whose field count mirrors an
/// external schema, not a decomposable design smell. Walks the file root because
/// the `impl` block is a sibling of the `struct_item`, not a child.
fn has_inherent_methods(item: tree_sitter::Node, source: &[u8], type_name: &str) -> bool {
    let mut root = item;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "impl_item"
            && node.child_by_field_name("trait").is_none()
            && impl_target_base_name(node, source) == Some(type_name)
            && impl_has_function(node)
        {
            return true;
        }
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

/// The base type name an inherent `impl` targets, unwrapping a `generic_type`
/// (`impl<T> Foo<T>`) to its base `Foo` so a generic struct's methods still
/// match its declaration. A plain `type_identifier` (`impl Foo`) is returned as
/// is.
fn impl_target_base_name<'a>(impl_item: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut target = impl_item.child_by_field_name("type")?;
    if target.kind() == "generic_type" {
        target = target.child_by_field_name("type")?;
    }
    target.utf8_text(source).ok()
}

/// True when the `impl_item`'s declaration body contains a `function_item`.
/// Methods are direct children of the impl body (`declaration_list`), so this
/// scans only the body's immediate children — deliberately shallow, to avoid
/// matching a `function_item` nested inside an unrelated item in the block.
fn impl_has_function(impl_item: tree_sitter::Node) -> bool {
    let Some(body) = impl_item.child_by_field_name("body") else {
        return false;
    };
    let mut cursor = body.walk();
    body.children(&mut cursor).any(|child| child.kind() == "function_item")
}

crate::ast_check! { on ["struct_item"] prefilter = ["struct"] => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "field_declaration_list" { return; }
    if derives_clap_cli(node, source) { return; }

    let name = node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .unwrap_or("?");

    if !has_inherent_methods(node, source, name) { return; }

    let mut count = 0usize;
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "field_declaration" {
            count += 1;
        }
    }

    if count <= MAX_FIELDS { return; }

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

    /// A field-heavy struct paired with an inherent method — a genuine god
    /// object (concentrated state AND behavior) that must still flag.
    fn make_struct(field_count: usize) -> String {
        format!("{}\nimpl Big {{ fn touch(&self) {{}} }}\n", make_struct_no_methods(field_count))
    }

    /// A field-heavy struct with no inherent methods — a pure data record / DTO
    /// that must be exempt.
    fn make_struct_no_methods(field_count: usize) -> String {
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
    fn allows_data_record_without_methods() {
        // No inherent `impl` block — a pure data carrier, not a god object.
        assert!(run(&make_struct_no_methods(30)).is_empty());
    }

    #[test]
    fn allows_data_record_with_only_trait_impl() {
        // A trait `impl` is not inherent behavior; the struct is still a DTO.
        let src = format!(
            "{}\nimpl Default for Big {{ fn default() -> Self {{ todo!() }} }}\n",
            make_struct_no_methods(30)
        );
        assert!(run(&src).is_empty());
    }

    #[test]
    fn allows_config_dto_mirroring_external_schema_issue5698() {
        // pnpm/pacquet `Config`: ~100 fields, one per .npmrc option, no inherent
        // methods — a serde-style config DTO mirroring a flat external format.
        let fields: String = (0..103).map(|i| format!("    opt{i}: bool,\n")).collect();
        let src = format!("#[derive(Debug, SmartDefault)]\npub struct Config {{\n{fields}}}\n");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn flags_data_record_once_it_gains_methods() {
        // Same field count, now with inherent behavior — a genuine god object.
        let src = format!(
            "{}\nimpl Config {{ fn merge(&self) {{}} }}\n",
            {
                let fields: String = (0..30).map(|i| format!("    opt{i}: bool,\n")).collect();
                format!("pub struct Config {{\n{fields}}}")
            }
        );
        let diags = run(&src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Config"));
    }

    #[test]
    fn flags_generic_god_object_with_methods() {
        // `impl<T> Big<T>` targets a `generic_type`; the gate must unwrap it to
        // the base name `Big` so a field-heavy generic struct with behavior is
        // still flagged rather than mistaken for a method-less DTO.
        let fields: String = (0..30).map(|i| format!("    f{i}: T,\n")).collect();
        let src = format!(
            "struct Big<T> {{\n{fields}}}\nimpl<T> Big<T> {{ fn touch(&self) {{}} }}\n"
        );
        let diags = run(&src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Big"));
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
