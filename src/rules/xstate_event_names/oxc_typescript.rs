//! xstate-event-names OXC backend — enforce SCREAMING_SNAKE_CASE on event keys
//! inside `on: { ... }` maps.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ObjectPropertyKind, PropertyKey};
use oxc_span::Span;
use std::sync::Arc;

pub struct Check;

/// Strip matching surrounding quote characters from a property key.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
}

/// SCREAMING_SNAKE_CASE: starts with A-Z, contains only A-Z, 0-9, underscore.
fn is_screaming_snake(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_uppercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// Extract the name and span from a PropertyKey.
fn key_name_and_span<'a>(key: &'a PropertyKey<'a>, _source: &'a str) -> Option<(String, Span)> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some((id.name.to_string(), id.span)),
        PropertyKey::StringLiteral(s) => Some((s.value.to_string(), s.span)),
        PropertyKey::NumericLiteral(n) => Some((n.raw.as_ref().map(|r| r.to_string()).unwrap_or_default(), n.span)),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["xstate."])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectExpression(obj) = node.kind() else {
            return;
        };

        // Check if this object is the value of an `on:` property.
        let parent = semantic.nodes().parent_node(node.id());
        let AstKind::ObjectProperty(on_prop) = parent.kind() else {
            return;
        };
        let parent_key = match &on_prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if parent_key != "on" {
            return;
        }

        // Iterate the keys of this object.
        for prop in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(p) = prop else {
                continue;
            };
            let Some((event_name, key_span)) = key_name_and_span(&p.key, ctx.source) else {
                continue;
            };

            let event_name = unquote(&event_name);
            if event_name.is_empty() {
                continue;
            }
            // Allow wildcard and xstate built-in lifecycle events.
            if event_name == "*" || event_name.starts_with("xstate.") {
                continue;
            }
            if is_screaming_snake(event_name) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, key_span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Event name `{event_name}` should be SCREAMING_SNAKE_CASE (e.g. `FETCH_DATA`)."
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
    fn allows_screaming_snake_events() {
        let src = r#"
            createMachine({
                on: {
                    NEXT: 'b',
                    FETCH_DATA: { target: 'loading' },
                    DONE_2: 'idle',
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn flags_camel_case_event() {
        let src = r#"
            createMachine({
                on: {
                    fetchData: 'loading',
                },
            });
        "#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("fetchData"));
    }


    #[test]
    fn flags_kebab_case_event() {
        let src = r#"
            createMachine({
                on: {
                    'fetch-data': 'loading',
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_wildcard_and_xstate_builtins() {
        let src = r#"
            createMachine({
                on: {
                    '*': 'fallback',
                    'xstate.init': 'starting',
                    'xstate.done.actor.foo': 'ok',
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_keys_outside_on() {
        let src = r#"
            const cfg = {
                states: {
                    idle: { entry: 'log' },
                },
            };
        "#;
        assert!(run_on(src).is_empty());
    }
}
