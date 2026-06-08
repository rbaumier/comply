//! ts-consistent-generic-constructors backend — default "constructor" mode:
//! flag `const x: Foo<T> = new Foo()` patterns where the type argument is on
//! the annotation rather than the constructor.
//!
//! Tree-sitter structure:
//!   variable_declarator {
//!     name: identifier,
//!     type: type_annotation > generic_type { identifier, type_arguments },
//!     value: new_expression { identifier }    // no type_arguments
//!   }

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["variable_declarator"] => |node, source, ctx, diagnostics|
    // Must have a `new` expression as value.
    let Some(value) = node.child_by_field_name("value") else {
        return;
    };
    if value.kind() != "new_expression" {
        return;
    }

    // The new expression must NOT have type arguments.
    let new_has_type_args = (0..value.named_child_count()).any(|i| {
        value.named_child(i)
            .map(|c| c.kind() == "type_arguments")
            .unwrap_or(false)
    });
    if new_has_type_args {
        return;
    }

    // Must have a type annotation with type_arguments.
    let Some(type_ann) = node.child_by_field_name("type") else {
        return;
    };

    // The type annotation should contain a generic_type with type_arguments.
    let has_generic = has_descendant_kind(type_ann, "type_arguments", 3);
    if !has_generic {
        return;
    }

    // Verify the constructor name matches the type name.
    let constructor_name = value
        .named_child(0)
        .and_then(|c| {
            if c.kind() == "identifier" {
                std::str::from_utf8(&source[c.byte_range()]).ok()
            } else {
                None
            }
        });

    let type_name = find_type_name(type_ann, source);

    if let (Some(cn), Some(tn)) = (constructor_name, type_name)
        && cn != tn {
            return;
        }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-consistent-generic-constructors".into(),
        message: "Generic type arguments should be specified on the constructor, not the type annotation.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

fn has_descendant_kind(node: tree_sitter::Node, kind: &str, max_depth: usize) -> bool {
    if max_depth == 0 {
        return false;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == kind {
            return true;
        }
        if has_descendant_kind(child, kind, max_depth - 1) {
            return true;
        }
    }
    false
}

fn find_type_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    if node.kind() == "identifier" || node.kind() == "type_identifier" {
        return std::str::from_utf8(&source[node.byte_range()]).ok();
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(name) = find_type_name(child, source) {
            return Some(name);
        }
    }
    None
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_type_annotation_with_generics() {
        let diags = run_on("const m: Map<string, number> = new Map();");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("constructor"));
    }

    #[test]
    fn allows_generics_on_constructor() {
        let diags = run_on("const m = new Map<string, number>();");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_no_generics() {
        let diags = run_on("const m = new Map();");
        assert!(diags.is_empty());
    }
}
