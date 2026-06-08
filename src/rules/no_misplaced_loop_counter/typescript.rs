//! no-misplaced-loop-counter backend — flag `for` loops where the
//! condition and update clause use different variables.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["for_statement"] => |node, source, ctx, diagnostics|
    let Some(condition) = node.child_by_field_name("condition") else {
        return;
    };
    let Some(increment) = node.child_by_field_name("increment") else {
        return;
    };
    let cond_var = match extract_condition_var(condition, source) {
        Some(v) => v,
        None => return,
    };
    let upd_var = match extract_update_var(increment, source) {
        Some(v) => v,
        None => return,
    };
    if cond_var != upd_var {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-misplaced-loop-counter".into(),
            message: "`for` loop condition and update use different variables — likely a copy-paste bug.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Extract the identifier being compared in a binary expression condition.
fn extract_condition_var<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    // The condition is usually a binary_expression like `i < n`
    if node.kind() == "binary_expression" {
        let left = node.child_by_field_name("left")?;
        if left.kind() == "identifier" {
            return left.utf8_text(source).ok();
        }
    }
    // Fallback: get the text and parse
    let text = node.utf8_text(source).ok()?;
    let text = text.trim();
    for op in &["<=", ">=", "!=", "===", "!==", "<", ">"] {
        if let Some(pos) = text.find(op) {
            let before = text[..pos].trim();
            let ident = before
                .rsplit(|c: char| !c.is_alphanumeric() && c != '_' && c != '$')
                .next()?;
            if !ident.is_empty()
                && ident
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphabetic() || c == '_' || c == '$')
            {
                return Some(ident);
            }
        }
    }
    None
}

/// Extract the identifier being modified in the update expression.
fn extract_update_var<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    match node.kind() {
        "update_expression" => {
            // i++ / ++i / i-- / --i
            let Some(arg) = node.child_by_field_name("argument") else {
                // Fallback: find the identifier child
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "identifier" {
                        return child.utf8_text(source).ok();
                    }
                }
                return None;
            };
            arg.utf8_text(source).ok()
        }
        "augmented_assignment_expression" => {
            // i += 1
            let left = node.child_by_field_name("left")?;
            left.utf8_text(source).ok()
        }
        "sequence_expression" => {
            // Take the first expression in a comma-separated update
            let first = node.named_child(0)?;
            extract_update_var(first, source)
        }
        _ => {
            let text = node.utf8_text(source).ok()?;
            let text = text.trim();
            // Handle postfix/prefix
            if text.ends_with("++") || text.ends_with("--") {
                let ident = text[..text.len() - 2].trim();
                if !ident.is_empty() {
                    return Some(ident);
                }
            }
            if text.starts_with("++") || text.starts_with("--") {
                let ident = text[2..].trim();
                if !ident.is_empty() {
                    return Some(ident);
                }
            }
            None
        }
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
    fn flags_different_vars() {
        assert_eq!(run_on("for (let i = 0; i < n; j++) {}").len(), 1);
    }

    #[test]
    fn flags_plus_equals_mismatch() {
        assert_eq!(run_on("for (let i = 0; i < n; j += 1) {}").len(), 1);
    }

    #[test]
    fn allows_matching_vars() {
        assert!(run_on("for (let i = 0; i < n; i++) {}").is_empty());
    }

    #[test]
    fn allows_matching_prefix() {
        assert!(run_on("for (let i = 0; i < 10; ++i) {}").is_empty());
    }
}
