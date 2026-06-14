//! ts-no-extraneous-class backend — flag classes that are empty, contain
//! only a constructor, or contain only static members (used as namespaces).
//!
//! Skips classes that extend a superclass.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["class_declaration", "class"] => |node, source, ctx, diagnostics|    // Skip classes with a superclass (extends)
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "class_heritage" || child.kind() == "extends_clause"
            || child.kind() == "decorator"
        {
            return;
        }
    }
    if let Some(parent) = node.parent() {
        let mut pc = parent.walk();
        if parent.named_children(&mut pc).any(|c| c.kind() == "decorator") {
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
    fn allows_decorated_empty_class() {
        assert!(run_on("@Component\nclass Foo {}").is_empty());
    }

    #[test]
    fn allows_exported_decorated_empty_class() {
        assert!(run_on("@Module({})\nexport class AppModule {}").is_empty());
    }

    // Regression for rbaumier/comply#2303 — empty stub/mock/token classes in
    // test files are idiomatic (Angular DI tokens, component stubs), so the
    // central `skip_in_test_dir` gate suppresses the rule for any file in a
    // test directory.
    #[test]
    fn gated_no_fp_on_empty_stub_class_in_spec_file() {
        let src = "describe('x', () => { class MockComponent {} class MockService {} });";
        assert!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                src,
                "src/lib/router-tree.spec.ts",
            )
            .is_empty(),
            "skip_in_test_dir must suppress empty stub classes in spec files"
        );
    }

    // The same empty class at a production path must still fire — the exemption
    // is test-directory-specific, not a blanket disable.
    #[test]
    fn gated_still_fires_on_empty_class_outside_test_directory() {
        let src = "class Empty {}";
        assert_eq!(
            crate::rules::test_helpers::run_rule_gated(&Check, src, "src/lib/widget.ts").len(),
            1,
            "the rule must still fire on production paths"
        );
    }
}
