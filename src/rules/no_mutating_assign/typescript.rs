//! no-mutating-assign backend — flag `Object.assign(target, ...)` where
//! `target` is not an empty object literal.
//!
//! `Object.assign(foo, src)` mutates `foo` in place, which is surprising
//! for callers holding references to `foo`. The idiomatic non-mutating
//! forms are `{...foo, ...src}` or `Object.assign({}, foo, src)`.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns true when `node` is an object literal with no properties (`{}`).
fn is_empty_object_literal(node: tree_sitter::Node) -> bool {
    if node.kind() != "object" {
        return false;
    }
    node.named_child_count() == 0
}

/// True when `name` is bound to a function expression, arrow function, or
/// function declaration in an ancestor scope. Used to allow
/// `Object.assign(fn, { displayName })` — attaching metadata to a function.
fn is_function_binding(start: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut ancestor = start.parent();
    while let Some(scope) = ancestor {
        let mut cursor = scope.walk();
        for child in scope.named_children(&mut cursor) {
            if node_binds_function(child, source, name) {
                return true;
            }
            if child.kind() == "export_statement"
                && let Some(decl) = child.child_by_field_name("declaration")
                && node_binds_function(decl, source, name)
            {
                return true;
            }
        }
        ancestor = scope.parent();
    }
    false
}

fn node_binds_function(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    if matches!(
        node.kind(),
        "function_declaration" | "generator_function_declaration"
    ) {
        return node
            .child_by_field_name("name")
            .map_or(false, |id| id.utf8_text(source).unwrap_or("") == name);
    }
    if matches!(node.kind(), "lexical_declaration" | "variable_declaration") {
        let mut cursor = node.walk();
        return node.named_children(&mut cursor).any(|decl| {
            if decl.kind() != "variable_declarator" {
                return false;
            }
            let Some(pat) = decl.child_by_field_name("name") else {
                return false;
            };
            if pat.kind() != "identifier" || pat.utf8_text(source).unwrap_or("") != name {
                return false;
            }
            decl.child_by_field_name("value").map_or(false, |v| {
                matches!(
                    v.kind(),
                    "arrow_function" | "function_expression" | "generator_function"
                )
            })
        });
    }
    false
}

/// True when `name` is a formal parameter of the enclosing function.
fn is_param_binding(start: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut ancestor = start.parent();
    while let Some(node) = ancestor {
        if matches!(
            node.kind(),
            "function_declaration"
                | "function_expression"
                | "method_definition"
                | "arrow_function"
        ) {
            if let Some(params) = node.child_by_field_name("parameters") {
                let mut cursor = params.walk();
                for param in params.named_children(&mut cursor) {
                    let pattern = match param.kind() {
                        "required_parameter" | "optional_parameter" => {
                            param.child_by_field_name("pattern")
                        }
                        "identifier" => Some(param),
                        _ => None,
                    };
                    if let Some(pat) = pattern {
                        if pat.kind() == "identifier"
                            && pat.utf8_text(source).unwrap_or("") == name
                        {
                            return true;
                        }
                    }
                }
            }
            // Arrow function with a single bare-identifier parameter: `x => body`
            if let Some(param) = node.child_by_field_name("parameter") {
                if param.kind() == "identifier" && param.utf8_text(source).unwrap_or("") == name {
                    return true;
                }
            }
        }
        ancestor = node.parent();
    }
    false
}

crate::ast_check! { on ["call_expression"] prefilter = ["Object.assign"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    // Callee must be exactly `Object.assign`.
    let obj = callee.child_by_field_name("object");
    let prop = callee.child_by_field_name("property");
    let Some(obj) = obj else { return };
    let Some(prop) = prop else { return };
    if obj.utf8_text(source).unwrap_or("") != "Object" {
        return;
    }
    if prop.utf8_text(source).unwrap_or("") != "assign" {
        return;
    }

    // Need at least one argument — the mutation target.
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first) = args.named_children(&mut args.walk()).next() else { return };

    // An empty object literal target (`Object.assign({}, ...)`) is the
    // non-mutating pattern — allow it.
    if is_empty_object_literal(first) {
        return;
    }

    // `Object.assign(fn, { displayName })` — attaching metadata to a function
    // identifier is not a data mutation, allow it.
    if first.kind() == "identifier" {
        let name = first.utf8_text(source).unwrap_or("");
        if is_function_binding(node, source, name) {
            return;
        }
        // `Object.assign(param, { begin })` — patching a library-owned object
        // passed as a parameter is the only option when no constructor is
        // accessible. Require the source to be a fresh object literal so that
        // `Object.assign(cfg, updates)` (two variables) still fires.
        let second = args.named_children(&mut args.walk()).nth(1);
        if second.map_or(false, |s| s.kind() == "object") && is_param_binding(node, source, name) {
            return;
        }
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-mutating-assign",
        "`Object.assign()` with a non-empty target mutates the target in place — use `{...target, ...source}` or `Object.assign({}, target, source)` instead.".into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_identifier_target() {
        assert_eq!(run_on("Object.assign(foo, bar);").len(), 1);
    }

    #[test]
    fn flags_non_empty_object_literal_target() {
        assert_eq!(run_on("Object.assign({ a: 1 }, bar);").len(), 1);
    }

    #[test]
    fn flags_member_expression_target() {
        assert_eq!(run_on("Object.assign(this.state, patch);").len(), 1);
    }

    #[test]
    fn allows_empty_object_target() {
        assert!(run_on("const merged = Object.assign({}, foo, bar);").is_empty());
    }

    #[test]
    fn ignores_other_calls() {
        assert!(run_on("assign(foo, bar);").is_empty());
    }

    #[test]
    fn ignores_unrelated_object_method() {
        assert!(run_on("Object.keys(foo);").is_empty());
    }

    #[test]
    fn ignores_no_arguments() {
        assert!(run_on("Object.assign();").is_empty());
    }

    // === Function target (issue #364) ===

    #[test]
    fn allows_arrow_function_target() {
        // Attaching metadata to a named handler — not a data mutation.
        assert!(run_on(
            r#"const handler = async (ctx) => { return ctx.body; };
Object.assign(handler, { displayName: "myHandler" });"#
        )
        .is_empty());
    }

    #[test]
    fn allows_function_declaration_target() {
        assert!(run_on(
            r#"function handler(ctx) { return ctx.body; }
Object.assign(handler, { displayName: "myHandler" });"#
        )
        .is_empty());
    }

    #[test]
    fn still_flags_plain_object_identifier() {
        // No function binding in scope — must still be flagged.
        assert_eq!(run_on("Object.assign(foo, bar);").len(), 1);
    }

    // === Parameter target with literal source (issue #583) ===

    #[test]
    fn allows_parameter_target_with_literal_source() {
        // Regression for #583 — patching a library instance passed as a
        // parameter is the only option when no constructor is accessible.
        let src = r#"
export function patchReservedBegin(reserved: ReservedSql): ReservedSql {
    const begin = async (...args: unknown[]): Promise<unknown> => {};
    Object.assign(reserved, { begin });
    return reserved;
}
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_parameter_with_variable_source() {
        // Two-variable merge — still a mutation smell.
        let src = r#"
function merge(target: Config, updates: Partial<Config>): Config {
    Object.assign(target, updates);
    return target;
}
        "#;
        assert_eq!(run_on(src).len(), 1);
    }
}
