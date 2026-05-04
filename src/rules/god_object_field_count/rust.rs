use crate::diagnostic::{Diagnostic, Severity};

const MAX_FIELDS: usize = 15;

crate::ast_check! { on ["struct_item"] prefilter = ["struct"] => |node, source, ctx, diagnostics|
    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "field_declaration_list" { return; }

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
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(s, &Check)
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
}
