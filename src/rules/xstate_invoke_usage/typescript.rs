//! xstate-invoke-usage backend — validate that `invoke: { ... }` entries
//! contain a `src` and only use the known invoke keys.
//!
//! Shape accepted:
//! - `invoke: { src: ..., ... }` — a single invoke object.
//! - `invoke: [{ src: ... }, { src: ... }]` — multiple invocations.
//!
//! Any other value type (string, number, function) is rejected.
//!
//! For each invoke object we verify:
//! - A `src` key is present.
//! - Any other key is in the known whitelist (typos like `onDon` get caught).

use crate::diagnostic::{Diagnostic, Severity};

/// All properties XState accepts on an invoke object.
const VALID_INVOKE_PROPS: &[&str] =
    &["src", "id", "input", "onDone", "onError", "onSnapshot", "systemId", "autoForward"];

/// Strip matching surrounding quote characters from a property key.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
}

/// Validate one invoke object: push diagnostics for missing `src` and for
/// any unknown keys.
fn validate_invoke_object(
    obj: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut has_src = false;
    let mut walker = obj.walk();
    for child in obj.named_children(&mut walker) {
        if child.kind() != "pair" {
            continue;
        }
        let Some(key_node) = child.child_by_field_name("key") else { continue };
        let key = unquote(key_node.utf8_text(source).unwrap_or(""));
        if key == "src" {
            has_src = true;
        }
        if !VALID_INVOKE_PROPS.contains(&key) {
            let pos = key_node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{key}` is not a valid `invoke` property (allowed: {}).",
                    VALID_INVOKE_PROPS.join(", ")
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
    if !has_src {
        let pos = obj.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: super::META.id.into(),
            message: "`invoke` object is missing required `src` property.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key_node) = node.child_by_field_name("key") else { return };
    if unquote(key_node.utf8_text(source).unwrap_or("")) != "invoke" {
        return;
    }
    let Some(value_node) = node.child_by_field_name("value") else { return };

    match value_node.kind() {
        "object" => {
            validate_invoke_object(value_node, source, ctx, diagnostics);
        }
        "array" => {
            let mut walker = value_node.walk();
            for child in value_node.named_children(&mut walker) {
                if child.kind() == "object" {
                    validate_invoke_object(child, source, ctx, diagnostics);
                }
            }
        }
        other => {
            let pos = value_node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "`invoke` must be an object or array of objects — got `{other}`."
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
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn allows_invoke_with_src() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        invoke: {
                            src: 'fetchData',
                            onDone: 'success',
                            onError: 'failure',
                        },
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_invoke_array() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        invoke: [
                            { src: 'a' },
                            { src: 'b', onDone: 'ok' },
                        ],
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_missing_src() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        invoke: { onDone: 'success' },
                    },
                },
            });
        "#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("missing required `src`"));
    }

    #[test]
    fn flags_unknown_invoke_prop() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        invoke: { src: 'fetchData', onDon: 'ok' },
                    },
                },
            });
        "#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("onDon"));
    }

    #[test]
    fn flags_invoke_as_string() {
        let src = r#"
            createMachine({
                states: {
                    loading: { invoke: 'fetchData' },
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_all_known_invoke_props() {
        let src = r#"
            createMachine({
                states: {
                    loading: {
                        invoke: {
                            src: 'fetch',
                            id: 'x',
                            input: {},
                            onDone: 'ok',
                            onError: 'err',
                            onSnapshot: 'snap',
                            systemId: 'sys',
                            autoForward: true,
                        },
                    },
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }
}
