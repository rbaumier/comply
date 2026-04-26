//! consistent-destructuring backend for TypeScript / JavaScript / TSX.
//!
//! Flags member expressions like `user.age` when the same object (`user`) was
//! already destructured earlier in the same scope: `const { name } = user;`.
//! The fix is to destructure `age` as well.
//!
//! Skips:
//! - Computed member expressions (`user[key]`)
//! - Method calls (`user.greet()`)
//! - Assignments to the property (`user.age = 5`)
//! - Nested member expressions on the result (`user.address.city`)

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

/// Extract text from source for a given node.
fn node_text<'a>(source: &'a str, node: &tree_sitter::Node) -> &'a str {
    &source[node.byte_range()]
}

/// Visit-time state. Destructuring declarations and member-expression
/// candidates are both gathered during the single walk, then matched in
/// `finish` so we don't depend on traversal order between sibling subtrees.
#[derive(Default)]
struct State {
    /// (object_text, end_byte_of_declaration)
    destructured: Vec<(String, usize)>,
    candidates: Vec<MemberCandidate>,
}

struct MemberCandidate {
    obj_text: String,
    prop_text: String,
    start_byte: usize,
    line: usize,
    column: usize,
}

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["variable_declarator", "member_expression"])
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(State::default()))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source;
        let Some(state) = state.and_then(|s| s.downcast_mut::<State>()) else {
            return;
        };

        if node.kind() == "variable_declarator" {
            // Match: `const { a, b } = expr;`
            // tree-sitter: variable_declarator with name=object_pattern, value=identifier/member_expression
            let pattern_node = match node.child_by_field_name("name") {
                Some(n) if n.kind() == "object_pattern" => n,
                _ => return,
            };

            let value_node = match node.child_by_field_name("value") {
                Some(n) => n,
                None => return,
            };

            // Only simple expressions: identifiers and member access chains.
            if !is_simple_expression(&value_node) {
                return;
            }

            let object_text = node_text(source, &value_node).to_string();

            // Collect destructured property names (only simple `{ a, b }` — skip
            // rest elements, computed props, nested patterns).
            let mut props = Vec::new();
            let mut has_rest = false;
            let child_count = pattern_node.named_child_count();
            for i in 0..child_count {
                let child = pattern_node.named_child(i).unwrap();
                if child.kind() == "rest_pattern" {
                    has_rest = true;
                    continue;
                }
                if child.kind() == "shorthand_property_identifier_pattern"
                    || child.kind() == "shorthand_property_identifier"
                {
                    props.push(node_text(source, &child).to_string());
                } else if child.kind() == "pair_pattern"
                    && let Some(key) = child.child_by_field_name("key")
                        && (key.kind() == "property_identifier" || key.kind() == "identifier") {
                            props.push(node_text(source, &key).to_string());
                        }
            }

            if props.is_empty() {
                return;
            }

            let end_byte = node.end_byte();

            // Don't flag if rest element is present — adding more destructured
            // props would change the rest object shape.
            if has_rest {
                return;
            }

            state.destructured.push((object_text, end_byte));
            return;
        }

        // member_expression
        if let Some(obj) = node.child_by_field_name("object")
            && let Some(prop) = node.child_by_field_name("property") {
                // Skip if parent is a member_expression (nested: `user.address.city`).
                if let Some(parent) = node.parent() {
                    if parent.kind() == "member_expression"
                        && let Some(parent_obj) = parent.child_by_field_name("object")
                            && parent_obj.id() == node.id() {
                                // This node is the object of a deeper access — skip.
                                return;
                            }
                    // Skip if this is the callee of a call (`user.greet()`)
                    if parent.kind() == "call_expression"
                        && let Some(callee) = parent.child_by_field_name("function")
                            && callee.id() == node.id() {
                                return;
                            }
                    // Skip assignments (`user.age = 5`)
                    if parent.kind() == "assignment_expression"
                        && let Some(left) = parent.child_by_field_name("left")
                            && left.id() == node.id() {
                                return;
                            }
                    // Skip augmented assignments (`user.age += 1`)
                    if parent.kind() == "augmented_assignment_expression"
                        && let Some(left) = parent.child_by_field_name("left")
                            && left.id() == node.id() {
                                return;
                            }
                }

                // Check if `[` follows object — computed access
                let obj_end = obj.end_byte();
                if obj_end < source.len() && source.as_bytes()[obj_end] == b'[' {
                    return;
                }

                let obj_text = node_text(source, &obj);
                let prop_text = node_text(source, &prop);
                let pos = node.start_position();

                state.candidates.push(MemberCandidate {
                    obj_text: obj_text.to_string(),
                    prop_text: prop_text.to_string(),
                    start_byte: node.start_byte(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                });
            }
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(state) = state.and_then(|s| s.downcast::<State>().ok()) else {
            return;
        };
        if state.destructured.is_empty() {
            return;
        }
        for c in &state.candidates {
            for (decl_obj, decl_end) in &state.destructured {
                if &c.obj_text == decl_obj && c.start_byte > *decl_end {
                    let prop_text = &c.prop_text;
                    let obj_text = &c.obj_text;
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: c.line,
                        column: c.column,
                        rule_id: "consistent-destructuring".into(),
                        message: format!(
                            "Use destructured variable for `{prop_text}` instead of `{obj_text}.{prop_text}`."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    break;
                }
            }
        }
    }
}

/// Check if a node is a "simple" expression (identifier or non-computed
/// member expression chain).
fn is_simple_expression(node: &tree_sitter::Node) -> bool {
    match node.kind() {
        "identifier" | "this" => true,
        "member_expression" => {
            if let Some(obj) = node.child_by_field_name("object") {
                // Check it's not computed (no `[` after object)
                is_simple_expression(&obj)
            } else {
                false
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_property_access_after_destructuring() {
        let diags = run_on(
            "const { name } = user;\nconsole.log(user.age);",
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("age"));
    }

    #[test]
    fn allows_fully_destructured() {
        assert!(run_on("const { name, age } = user;\nconsole.log(name, age);").is_empty());
    }

    #[test]
    fn allows_different_object() {
        assert!(run_on("const { name } = user;\nconsole.log(other.age);").is_empty());
    }

    #[test]
    fn skips_method_calls() {
        assert!(run_on("const { name } = user;\nuser.greet();").is_empty());
    }

    #[test]
    fn skips_assignment_to_property() {
        assert!(run_on("const { name } = user;\nuser.age = 5;").is_empty());
    }

    #[test]
    fn skips_computed_access() {
        assert!(run_on("const { name } = user;\nconsole.log(user[key]);").is_empty());
    }

    #[test]
    fn skips_rest_destructuring() {
        assert!(run_on("const { name, ...rest } = user;\nconsole.log(user.age);").is_empty());
    }

    #[test]
    fn skips_nested_member() {
        // user.address.city — nested access, don't flag
        assert!(run_on("const { name } = user;\nconsole.log(user.address.city);").is_empty());
    }
}
