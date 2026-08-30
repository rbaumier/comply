//! comment-requires-tag — Vue SFC backend.
//!
//! Covers both halves of the file.
//! The template's `<!-- -->` comments, and every `<script>` block's own.

use crate::diagnostic::Diagnostic;
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::comment_blocks;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        super::diagnose(comment_blocks::from_vue_sfc(tree, ctx.source), ctx)
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.vue")
    }

    #[test]
    fn flags_untagged_template_comment() {
        let source = "<template>\n  <!-- the header row -->\n  <div />\n</template>";
        let diagnostics = run(source);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 2);
    }

    #[test]
    fn allows_tagged_template_comment() {
        let source = "<template>\n  <!-- why: the slot is rendered by the parent -->\n  <div />\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_untagged_script_comment_in_file_coordinates() {
        let source = "<template>\n  <div />\n</template>\n<script setup lang=\"ts\">\n// fetch the session\nconst s = load();\n</script>";
        let diagnostics = run(source);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].line, 5);
    }

    #[test]
    fn allows_tagged_script_comment() {
        let source = "<script setup>\n// why: the store is hydrated before mount\nconst s = load();\n</script>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_a_component_with_no_comment() {
        assert!(run("<template>\n  <div />\n</template>").is_empty());
    }
}
