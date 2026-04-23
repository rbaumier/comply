//! no-empty-catch (TS/JS/TSX) — flag `catch (e) {}` with an empty body.
//!
//! Detects `catch_clause` whose `statement_block` has zero named children.
//! A body containing only comments has zero named children in tree-sitter,
//! so we additionally allow the block if the raw text between braces is
//! non-whitespace (i.e. contains a comment).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "catch_clause" {
        return;
    }

    let Some(body) = node.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" {
        return;
    }

    if body.named_child_count() != 0 {
        return;
    }

    // Allow empty catch blocks that contain at least one comment.
    if block_has_comment(&body, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-empty-catch".into(),
        message: "Empty catch block silently swallows the error — log it, rethrow, \
                  or add a comment explaining why."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

fn block_has_comment(block: &tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = block.walk();
    for child in block.children(&mut cursor) {
        if child.kind() == "comment" {
            return true;
        }
    }
    // Fallback: check raw text between `{` and `}` for a `//` or `/*` marker.
    let text = block.utf8_text(source).unwrap_or("");
    text.contains("//") || text.contains("/*")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_empty_catch() {
        let d = run_on("try { x(); } catch (e) {}");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("swallows"));
    }

    #[test]
    fn flags_empty_catch_without_binding() {
        let d = run_on("try { x(); } catch {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_non_empty_catch() {
        assert!(run_on("try { x(); } catch (e) { log(e); }").is_empty());
    }

    #[test]
    fn allows_catch_with_comment() {
        assert!(run_on("try { x(); } catch (e) { /* intentional */ }").is_empty());
    }
}
