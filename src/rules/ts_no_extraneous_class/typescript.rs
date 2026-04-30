//! ts-no-extraneous-class backend — flag classes that are empty, contain
//! only a constructor, or contain only static members (used as namespaces).
//!
//! Skips classes that extend a superclass.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["class_declaration", "class"] => |node, source, ctx, diagnostics|    // Skip classes with a superclass (extends)
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "class_heritage" || child.kind() == "extends_clause" {
            return;
        }
    }
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    let mut body_cursor = body.walk();
    let members: Vec<_> = body.named_children(&mut body_cursor).collect();

    if members.is_empty() {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-no-extraneous-class".into(),
            message: "Unexpected empty class.".into(),
            severity: Severity::Warning,
            span: None,
        });
        return;
    }

    let mut only_constructor = true;
    let mut only_static = true;

    for member in &members {
        let mk = member.kind();
        if mk == "method_definition" {
            // Check if it's a constructor
            let is_ctor = member
                .child_by_field_name("name")
                .map(|n| &source[n.byte_range()] == b"constructor")
                .unwrap_or(false);
            if is_ctor {
                // Check for parameter properties (accessibility modifier on params)
                if let Some(params) = member.child_by_field_name("parameters") {
                    let mut pc = params.walk();
                    for param in params.named_children(&mut pc) {
                        if param.kind() == "required_parameter" {
                            let mut cc = param.walk();
                            for child in param.children(&mut cc) {
                                if child.kind() == "accessibility_modifier" {
                                    only_constructor = false;
                                    only_static = false;
                                }
                            }
                        }
                    }
                }
            } else {
                only_constructor = false;
                // Check if static
                let mut mc = member.walk();
                let is_static = member.children(&mut mc).any(|c| {
                    let ct = &source[c.byte_range()];
                    ct == b"static"
                });
                if !is_static {
                    only_static = false;
                }
            }
        } else if mk == "public_field_definition" || mk == "property_definition" {
            only_constructor = false;
            let mut mc = member.walk();
            let is_static = member.children(&mut mc).any(|c| {
                let ct = &source[c.byte_range()];
                ct == b"static"
            });
            if !is_static {
                only_static = false;
            }
        } else {
            only_constructor = false;
            only_static = false;
        }
        if !only_constructor && !only_static {
            break;
        }
    }

    let msg = if only_constructor {
        "Unexpected class with only a constructor."
    } else if only_static {
        "Unexpected class with only static properties."
    } else {
        return;
    };
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-extraneous-class".into(),
        message: msg.into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_empty_class() {
        let diags = run_on("class Empty {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("empty"));
    }

    #[test]
    fn flags_only_static() {
        let diags = run_on("class Utils { static foo() {} }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("static"));
    }

    #[test]
    fn allows_class_with_extends() {
        assert!(run_on("class Foo extends Bar {}").is_empty());
    }

    #[test]
    fn allows_class_with_instance_method() {
        assert!(run_on("class Foo { bar() {} }").is_empty());
    }

    #[test]
    fn flags_decorated_empty_class() {
        assert!(!run_on("@Component\nclass Foo {}").is_empty());
    }

    #[test]
    fn flags_exported_decorated_empty_class() {
        assert!(!run_on("@Module({})\nexport class AppModule {}").is_empty());
    }
}
