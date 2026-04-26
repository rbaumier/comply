use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

/// Returns `true` if `name` appears as an identifier anywhere inside the
/// subtree rooted at `node` (excluding the catch parameter itself).
fn is_identifier_used_in(node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    let mut found = false;

    // Depth-first walk
    loop {
        let n = cursor.node();
        if n.kind() == "identifier"
            && let Ok(text) = n.utf8_text(source)
                && text == name {
                    found = true;
                    break;
                }

        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return found;
            }
        }
    }
    found
}

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["catch_clause"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();

        // Find the catch parameter — tree-sitter names it "parameter"
        // In TS grammar: catch_clause -> "catch" "(" parameter ")" body
        let param_node = match node.child_by_field_name("parameter") {
            Some(n) => n,
            None => return, // Already omitted — `catch { ... }`
        };

        let param_text = match param_node.utf8_text(source) {
            Ok(t) => t,
            Err(_) => return,
        };

        // For destructuring / typed patterns, extract the identifier.
        // Common shapes:
        //   `error`           → Identifier
        //   `error: any`      → type_annotation parent (TS)
        //   `(error)`         → parenthesized
        // We just need the bare name for the usage scan.
        let param_name = param_text
            .split(':')
            .next()
            .unwrap_or(param_text)
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')');

        if param_name.is_empty() {
            return;
        }

        // Get the catch body (statement_block)
        let body = match node.child_by_field_name("body") {
            Some(b) => b,
            None => return,
        };

        // Check if the parameter name is used anywhere in the catch body
        if !is_identifier_used_in(body, param_name, source) {
            let pos = param_node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "prefer-optional-catch-binding".into(),
                message: format!(
                    "Unused catch binding `{param_name}`. Remove it: use `catch {{ … }}` instead of `catch ({param_name}) {{ … }}`."
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
    use crate::rules::test_helpers;

    fn run(source: &str) -> Vec<Diagnostic> {
        test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_unused_catch_parameter() {
        let d = run("try { foo(); } catch (error) { console.log('failed'); }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Unused catch binding"));
        assert!(d[0].message.contains("error"));
    }

    #[test]
    fn flags_unused_underscore_param() {
        let d = run("try { foo(); } catch (_) { handleError(); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_used_catch_parameter() {
        assert!(run("try { foo(); } catch (error) { console.log(error); }").is_empty());
    }

    #[test]
    fn allows_omitted_catch_binding() {
        assert!(run("try { foo(); } catch { console.log('failed'); }").is_empty());
    }

    #[test]
    fn allows_error_rethrown() {
        assert!(run("try { foo(); } catch (e) { throw e; }").is_empty());
    }

    #[test]
    fn flags_unused_in_empty_catch_body() {
        let d = run("try { risky(); } catch (err) { }");
        assert_eq!(d.len(), 1);
    }
}
