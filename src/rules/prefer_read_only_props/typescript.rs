//! prefer-read-only-props backend — React props should be Readonly<>.

use crate::diagnostic::{Diagnostic, Severity};

/// Check if a name starts with an uppercase letter (component convention).
fn is_component_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_uppercase())
}

/// Extract the type annotation text from a parameter node, if it has one
/// and it's not already `Readonly<…>` and not an inline object type `{…}`.
fn has_non_readonly_type_annotation(param: tree_sitter::Node, source: &[u8]) -> bool {
    // Look for `: Type` — tree-sitter uses "type_annotation" as field.
    let Some(type_ann) = param.child_by_field_name("type") else { return false };

    let type_text = type_ann.utf8_text(source).unwrap_or("").trim();

    // Skip if already Readonly
    if type_text.starts_with("Readonly<") || type_text.starts_with(": Readonly<") {
        return false;
    }

    // Get the actual type node (skip the `:` token).
    // The type_annotation node may contain a type_identifier or generic_type etc.
    let inner = type_ann.utf8_text(source).unwrap_or("");
    let trimmed = inner.trim().trim_start_matches(':').trim();

    if trimmed.is_empty() {
        return false;
    }

    // Skip inline object types
    if trimmed.starts_with('{') {
        return false;
    }

    // Skip Readonly
    if trimmed.starts_with("Readonly<") {
        return false;
    }

    true
}

crate::ast_check! { |node, source, ctx, diagnostics|
    match node.kind() {
        "function_declaration" | "function" => {
            // Check if function name starts with uppercase (component).
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");

            if !is_component_name(name) { return; }

            let Some(params) = node.child_by_field_name("parameters") else { return };
            check_params(params, source, ctx, diagnostics);
        }
        "variable_declarator" => {
            // `const MyComponent = (props: MyProps) => {`
            let name = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .unwrap_or("");

            if !is_component_name(name) { return; }

            // Value must be an arrow function.
            let Some(value) = node.child_by_field_name("value") else { return };
            if value.kind() != "arrow_function" { return; }

            let Some(params) = value.child_by_field_name("parameters") else { return };
            if params.kind() == "formal_parameters" {
                check_params(params, source, ctx, diagnostics);
            }
        }
        _ => {}
    }
}

fn check_params(
    params: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Must have at least one parameter.
    if params.named_child_count() == 0 { return; }

    // Check first param — props parameter.
    let first = params.named_child(0).unwrap();

    let has_violation = match first.kind() {
        "required_parameter" | "optional_parameter" => {
            has_non_readonly_type_annotation(first, source)
        }
        _ => false,
    };

    if has_violation {
        let pos = first.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "prefer-read-only-props".into(),
            message: "Props type should be wrapped in `Readonly<>` to prevent mutation.".into(),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_function_component_without_readonly() {
        assert_eq!(run_on("function MyComponent(props: MyProps) {}").len(), 1);
    }

    #[test]
    fn flags_destructured_props_without_readonly() {
        assert_eq!(
            run_on("function MyComponent({ name, age }: MyProps) {}").len(),
            1
        );
    }

    #[test]
    fn allows_readonly_props() {
        assert!(run_on("function MyComponent(props: Readonly<MyProps>) {}").is_empty());
    }

    #[test]
    fn ignores_non_component_functions() {
        assert!(run_on("function helper(data: MyType) {}").is_empty());
    }

    #[test]
    fn flags_arrow_component() {
        assert_eq!(
            run_on("const MyComponent = (props: MyProps) => {}").len(),
            1
        );
    }
}
