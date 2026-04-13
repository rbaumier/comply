//! catch-error-name Vue SFC backend.
//!
//! Extracts every `<script>` block from the Vue tree, re-parses its
//! body with the TypeScript grammar, and walks the inner tree for
//! `catch_clause` nodes. Diagnostic coordinates are translated from
//! the re-parsed inner tree back to the outer Vue file.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::vue_sfc::{self, ScriptBlock};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let blocks = vue_sfc::extract_scripts(tree, ctx.source);
        let mut diagnostics = Vec::new();
        for block in blocks {
            lint_block(&block, ctx, &mut diagnostics);
        }
        diagnostics
    }
}

fn lint_block(block: &ScriptBlock<'_>, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .is_err()
    {
        return;
    }
    let Some(inner_tree) = parser.parse(block.text, None) else {
        return;
    };

    let source_bytes = block.text.as_bytes();
    let mut cursor = inner_tree.root_node().walk();
    let mut stack: Vec<tree_sitter::Node> = vec![inner_tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "catch_clause" {
            inspect_catch(node, source_bytes, block, ctx, diagnostics);
        }
        cursor.reset(node);
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

fn inspect_catch(
    node: tree_sitter::Node,
    source: &[u8],
    block: &ScriptBlock<'_>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(param) = node.child_by_field_name("parameter") else { return };

    let ident = if param.kind() == "identifier" {
        param
    } else {
        match find_identifier(param) {
            Some(id) => id,
            None => return,
        }
    };

    let Ok(name) = ident.utf8_text(source) else { return };

    if super::is_acceptable_name(name) {
        return;
    }

    let pos = ident.start_position();
    let file_row = pos.row + block.start_row;
    let file_col = if pos.row == 0 {
        pos.column + block.start_column
    } else {
        pos.column
    };

    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: file_row + 1,
        column: file_col + 1,
        rule_id: "catch-error-name".into(),
        message: format!(
            "The catch parameter `{name}` should be named `{}`.",
            super::EXPECTED
        ),
        severity: Severity::Warning,
        span: None,
    });
}

fn find_identifier(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let count = node.named_child_count();
    for i in 0..count {
        if let Some(child) = node.named_child(i)
            && child.kind() == "identifier"
        {
            return Some(child);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::{Language, SourceFile};
    use std::path::PathBuf;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        let file = SourceFile {
            path: PathBuf::from("t.vue"),
            language: Language::Vue,
        };
        Check.check(
            &crate::rules::backend::CheckCtx::for_test(&file.path, source),
            &tree,
        )
    }

    #[test]
    fn flags_catch_e_in_vue_script() {
        let src = "<script>\ntry { f(); } catch (e) { log(e); }\n</script>";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`e`"));
    }

    #[test]
    fn flags_catch_err_in_script_setup() {
        let src = "<script setup lang=\"ts\">\ntry { f(); } catch (err) { throw err; }\n</script>";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`err`"));
    }

    #[test]
    fn allows_catch_error_in_vue_script() {
        let src = "<script>\ntry { f(); } catch (error) { log(error); }\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_suffixed_error_in_vue_script() {
        let src = "<script>\ntry { f(); } catch (parseError) {}\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_catch_in_vue_script() {
        let src = "<script>\ntry { f(); } catch {}\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_destructured_catch_in_vue_script() {
        let src = "<script>\ntry { f(); } catch ({ message }) {}\n</script>";
        assert!(run(src).is_empty());
    }
}
