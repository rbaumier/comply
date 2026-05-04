//! catch-error-name Vue SFC backend (oxc-based).
//!
//! Uses tree-sitter-vue to extract `<script>` blocks, then delegates
//! to `vue_sfc_oxc::run_oxc_check_on_vue_block` which parses each
//! block with oxc_parser and runs `oxc_typescript::Check`.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::{vue_sfc, vue_sfc_oxc};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let blocks = vue_sfc::extract_scripts(tree, ctx.source);
        let mut diagnostics = Vec::new();
        for block in &blocks {
            vue_sfc_oxc::run_oxc_check_on_vue_block(
                block,
                &super::oxc_typescript::Check,
                ctx,
                &mut diagnostics,
            );
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::Language;
    use crate::rules::backend::CheckCtx;
    use std::path::PathBuf;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parse");
        let path = PathBuf::from("t.vue");
        let ctx = CheckCtx::for_test(&path, source);
        Check.check(&ctx, &tree)
    }

    #[test]
    fn flags_catch_e_in_vue_script() {
        let src = "<script>\ntry { f(); } catch (e) { log(e); }\n</script>";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`e`"));
    }

    #[test]
    fn allows_catch_error_in_vue_script() {
        let src = "<script>\ntry { f(); } catch (error) { log(error); }\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_catch_in_vue_script() {
        let src = "<script>\ntry { f(); } catch {}\n</script>";
        assert!(run(src).is_empty());
    }
}
