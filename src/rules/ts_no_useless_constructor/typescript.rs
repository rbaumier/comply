//! ts-no-useless-constructor backend — flag constructors that are empty
//! or only call `super(...)` with the same arguments, and have no
//! accessibility modifiers, parameter properties, or decorators.
//!
//! Detection: walk `method_definition` nodes with name `constructor`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["method_definition"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    if &source[name_node.byte_range()] != b"constructor" {
        return;
    }
    // Skip if constructor has accessibility modifier (private/protected/public)
    let mut nc = node.walk();
    for child in node.children(&mut nc) {
        if child.kind() == "accessibility_modifier" {
            return; // access-restricted constructor is useful
        }
        if child.kind() == "override_modifier" {
            return;
        }
    }
    // Skip if any parameter has decorators or is a parameter property
    if let Some(params) = node.child_by_field_name("parameters") {
        let mut pc = params.walk();
        for param in params.named_children(&mut pc) {
            if param.kind() == "required_parameter" || param.kind() == "optional_parameter" {
                let mut cc = param.walk();
                for child in param.children(&mut cc) {
                    if child.kind() == "accessibility_modifier"
                        || child.kind() == "readonly"
                        || child.kind() == "decorator"
                    {
                        return; // parameter property or decorated param
                    }
                }
            }
            if param.kind() == "decorator" {
                return;
            }
        }
    }
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    if body.kind() != "statement_block" {
        return;
    }
    let mut body_cursor = body.walk();
    let stmts: Vec<_> = body.named_children(&mut body_cursor)
        .filter(|c| c.kind() != "comment")
        .collect();
    // Case 1: completely empty body
    if stmts.is_empty() {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-no-useless-constructor".into(),
            message: "Useless constructor — remove it.".into(),
            severity: Severity::Warning,
            span: None,
        });
        return;
    }
    // Case 2: only `super(...)` call with same args passthrough
    if stmts.len() != 1 {
        return;
    }
    let stmt = stmts[0];
    if stmt.kind() != "expression_statement" {
        return;
    }
    let mut sc = stmt.walk();
    let expr = stmt.named_children(&mut sc).next();
    let Some(call) = expr else {
        return;
    };
    if call.kind() != "call_expression" {
        return;
    }
    let Some(callee) = call.child_by_field_name("function") else {
        return;
    };
    if callee.kind() != "super" {
        return;
    }
    // Check arguments match parameter names
    let Some(call_args) = call.child_by_field_name("arguments") else {
        return;
    };
    let Some(params) = node.child_by_field_name("parameters") else {
        return;
    };
    let mut aac = call_args.walk();
    let arg_names: Vec<String> = call_args
        .named_children(&mut aac)
        .filter_map(|a| {
            if a.kind() == "identifier" {
                std::str::from_utf8(&source[a.byte_range()])
                    .ok()
                    .map(|s| s.trim().to_string())
            } else if a.kind() == "spread_element" {
                // ...args
                let mut sc2 = a.walk();
                a.named_children(&mut sc2).next().and_then(|inner| {
                    if inner.kind() == "identifier" {
                        std::str::from_utf8(&source[inner.byte_range()])
                            .ok()
                            .map(|s| format!("...{}", s.trim()))
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .collect();
    // Format param names with spread prefix if rest parameter
    let mut formatted_params: Vec<String> = Vec::new();
    let mut pac2 = params.walk();
    for p in params.named_children(&mut pac2) {
        if p.kind() == "rest_parameter" {
            if let Some(name_n) = p.child_by_field_name("pattern")
                .or_else(|| p.child_by_field_name("name"))
                && let Ok(s) = std::str::from_utf8(&source[name_n.byte_range()]) {
                    formatted_params.push(format!("...{}", s.trim()));
                }
        } else if let Some(name_n) = p.child_by_field_name("pattern")
            .or_else(|| p.child_by_field_name("name"))
            .or_else(|| if p.kind() == "identifier" { Some(p) } else { None })
            && let Ok(s) = std::str::from_utf8(&source[name_n.byte_range()]) {
                formatted_params.push(s.trim().to_string());
            }
    }
    if formatted_params == arg_names {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "ts-no-useless-constructor".into(),
            message: "Useless constructor — it only calls `super()` with the same arguments."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_empty_constructor() {
        let diags = run_on("class Foo { constructor() {} }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_constructor_with_body() {
        assert!(run_on("class Foo { constructor() { this.init(); } }").is_empty());
    }

    #[test]
    fn allows_private_constructor() {
        assert!(run_on("class Foo { private constructor() {} }").is_empty());
    }

    #[test]
    fn allows_parameter_property() {
        assert!(run_on("class Foo { constructor(public name: string) {} }").is_empty());
    }
}
