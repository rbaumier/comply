use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // Look for `this.prop = value` assignments
    if node.kind() != "assignment_expression" { return; }

    let Some(left) = node.child_by_field_name("left") else { return; };
    if left.kind() != "member_expression" { return; }

    let Some(obj) = left.child_by_field_name("object") else { return; };
    if obj.utf8_text(source).unwrap_or("") != "this" { return; }

    // Check if we're inside a constructor
    let mut current = node.parent();
    while let Some(parent) = current {
        match parent.kind() {
            "method_definition" => {
                if let Some(name) = parent.child_by_field_name("name")
                    && name.utf8_text(source).unwrap_or("") == "constructor" {
                        return; // Inside constructor, allowed
                    }
                break; // Inside a method but not constructor
            }
            "function_declaration" | "function_expression" | "arrow_function" => {
                break; // Inside a regular function, not a method
            }
            "class_body" => {
                // Direct assignment in class body (field initializer) is OK
                return;
            }
            _ => {}
        }
        current = parent.parent();
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-this-mutation".into(),
        message: "Mutation of `this` outside constructor — initialize properties in constructor.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(code: &str) -> Vec<Diagnostic> { crate::rules::test_helpers::run_ts(code, &Check) }

    #[test]
    fn flags_this_mutation_in_method() {
        let code = r#"
            class Foo {
                update() { this.value = 1; }
            }
        "#;
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_constructor_assignment() {
        let code = r#"
            class Foo {
                constructor() { this.value = 1; }
            }
        "#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_field_initializer() {
        let code = r#"
            class Foo {
                value = 1;
            }
        "#;
        assert!(run(code).is_empty());
    }

    #[test]
    fn flags_setter() {
        let code = r#"
            class Foo {
                set value(v) { this._value = v; }
            }
        "#;
        assert_eq!(run(code).len(), 1);
    }
}
