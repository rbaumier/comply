//! xstate-state-names OxcCheck backend — enforce camelCase or snake_case on
//! state keys inside a `states: { ... }` map.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// camelCase: lowercase first char, alphanumerics only.
fn is_camel_case(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else { return false };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric())
}

/// snake_case: lowercase first char, lowercase letters / digits / underscore only.
fn is_snake_case(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else { return false };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Strip matching surrounding quote characters from a property key.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectExpression(obj) = node.kind() else { return };

        // Check if this object is the value of a `states: { ... }` pair.
        // Walk up to parent: should be an ObjectProperty with key "states".
        let parent_id = semantic.nodes().parent_id(node.id());
        if parent_id == node.id() { return; }
        let parent = semantic.nodes().get_node(parent_id);
        let AstKind::ObjectProperty(prop) = parent.kind() else { return };

        let key_text = match &prop.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if key_text != "states" {
            return;
        }

        // Now check each property of this object.
        for property in &obj.properties {
            let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(inner) = property else {
                continue;
            };
            let name = match &inner.key {
                oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };
            let name = unquote(name);
            if name.is_empty() || is_camel_case(name) || is_snake_case(name) {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(ctx.source, inner.key.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "State name `{name}` should be camelCase or snake_case (e.g. `fetchingData`)."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn allows_camel_case_state() {
        let src = r#"
            createMachine({
                states: {
                    idle: {},
                    fetchingData: {},
                    ready2go: {},
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_snake_case_state() {
        let src = r#"
            createMachine({
                states: {
                    fetching_data: {},
                    step_1: {},
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_pascal_case_state() {
        let src = r#"
            createMachine({
                states: {
                    Idle: {},
                },
            });
        "#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Idle"));
    }

    #[test]
    fn flags_screaming_snake_state() {
        let src = r#"
            createMachine({
                states: {
                    IDLE_STATE: {},
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_keys_outside_states() {
        let src = r#"
            createMachine({
                on: { FETCH: 'loading' },
                context: { Count: 0 },
            });
        "#;
        assert!(run_on(src).is_empty());
    }
}
