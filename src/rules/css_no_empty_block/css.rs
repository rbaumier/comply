//! Flag style rules whose block holds no content.
//!
//! A block holds no content when its only named children are comments, either
//! `/* … */` or `//`. Every other named child is content: a declaration, a
//! nested rule set, or an at-rule statement. An `ERROR` child is content too.
//! Unreadable syntax is not evidence of emptiness, and reporting the block
//! would advise deleting text that may render.
//!
//! A comment-only block gets its own message. Comments render no style, so
//! deleting the rule drops no layout. `.a { /* TODO */ }` is the placeholder
//! this rule exists to catch. stylelint's `block-no-empty` needs
//! `ignore: ["comments"]` for the same finding.
//!
//! The check visits `rule_set`. An empty at-rule block such as
//! `@media print { }` is out of scope. So is an empty keyframe block such as
//! `from { }`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["rule_set"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let Some(block) = node.children(&mut c).find(|n| n.kind() == "block") else { return; };

    // `comment` and `js_comment` are the grammar's only named `extras`.
    let mut bc = block.walk();
    if block.named_children(&mut bc).any(|n| !matches!(n.kind(), "comment" | "js_comment")) {
        return;
    }

    let message = if block.named_child_count() == 0 {
        "Empty block; delete the rule or add its styles."
    } else {
        "Block holds only comments; delete the rule or add its styles."
    };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &block,
        super::META.id,
        message.into(),
        Severity::Error,
    ));
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.css")
    }

    #[test]
    fn flags_empty_block() {
        for src in [".a { }", ".a {}", ".a {\n}"] {
            let diags = run(src);
            assert_eq!(diags.len(), 1, "{src:?} -> {diags:?}");
            assert!(
                diags[0].message.contains("Empty block"),
                "{src:?} -> {}",
                diags[0].message
            );
        }
    }

    #[test]
    fn allows_block_with_declarations() {
        assert!(run(".a { color: red; }").is_empty());
    }

    #[test]
    fn allows_block_whose_only_content_is_a_nested_rule() {
        let diags = run(".card { & .title { color: red; } }");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn allows_block_whose_only_content_is_a_nested_pseudo_class_rule() {
        let diags = run(".a { &:hover { color: red; } }");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn allows_block_whose_only_content_is_an_at_rule() {
        let diags = run("body { @apply bg-background text-foreground; }");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn allows_block_whose_only_content_is_a_media_query() {
        let diags = run(".a { @media (width > 40rem) { color: red; } }");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn allows_block_whose_only_content_is_a_supports_query() {
        let diags = run(".a { @supports (display: grid) { display: grid; } }");
        assert!(diags.is_empty(), "{diags:?}");
    }

    /// A comment next to real content leaves the block populated: a comment
    /// child must not by itself make the block empty.
    #[test]
    fn allows_block_mixing_a_comment_with_a_declaration() {
        for src in [".a { /* c */ color: red; }", ".a { color: red; /* c */ }"] {
            let diags = run(src);
            assert!(diags.is_empty(), "{src:?} -> {diags:?}");
        }
    }

    #[test]
    fn flags_empty_block_nested_in_an_at_rule() {
        let diags = run("@media print {\n  .a { }\n}");
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].line, 2);
    }

    /// A nested rule is content even when that nested rule is itself empty, so
    /// the inner block is the only finding.
    #[test]
    fn flags_only_the_inner_block_of_an_empty_nested_rule() {
        let diags = run(".outer {\n  .inner { }\n}");
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn flags_empty_block_nested_beside_a_declaration() {
        let diags = run(".outer {\n  color: red;\n  .inner { }\n}");
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].line, 3);
    }

    #[test]
    fn flags_comment_only_block_and_says_so() {
        let diags = run(".a { /* nothing yet */ }");
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert!(
            diags[0].message.contains("only comments"),
            "{}",
            diags[0].message
        );
    }

    /// tree-sitter-css parses `//` into `js_comment`, a different kind from the
    /// `/* … */` `comment`. Both render nothing, so both leave a block empty.
    #[test]
    fn flags_line_comment_only_block_and_says_so() {
        for src in [".a {\n  // TODO\n}", ".a {\n  /* c1 */\n  // c2\n}"] {
            let diags = run(src);
            assert_eq!(diags.len(), 1, "{src:?} -> {diags:?}");
            assert!(
                diags[0].message.contains("only comments"),
                "{src:?} -> {}",
                diags[0].message
            );
        }
    }

    /// Unparsed syntax is content the grammar could not read, not emptiness:
    /// reporting it would advise deleting a block that may render. The custom
    /// property holds a block value, which is valid CSS the grammar cannot read.
    #[test]
    fn allows_block_the_grammar_fails_to_parse() {
        for src in [".a { ; }", ".a { --x: {a:b}; }"] {
            let diags = run(src);
            assert!(diags.is_empty(), "{src:?} -> {diags:?}");
        }
    }

    /// The check visits `rule_set`, so an at-rule's own block and a keyframe
    /// block stay outside its scope. This pins the boundary the docblock states.
    #[test]
    fn allows_empty_at_rule_and_keyframe_blocks() {
        for src in ["@media print { }", "@keyframes k { from { } to { opacity: 1; } }"] {
            let diags = run(src);
            assert!(diags.is_empty(), "{src:?} -> {diags:?}");
        }
    }

    /// The four blocks of `remeda/remeda`'s `packages/docs/src/styles/global.css`
    /// base layer; each holds a single `@apply` and paints the docs site.
    #[test]
    fn allows_remeda_tailwind_base_layer() {
        let src = r#"@layer base {
  * {
    @apply border-border outline-ring/50;
  }
  body {
    @apply bg-background text-foreground;
  }
  html {
    @apply font-sans;
  }
  h1,
  h2,
  h3,
  h4,
  h5,
  h6 {
    @apply font-heading;
  }
}"#;
        let diags = run(src);
        assert!(diags.is_empty(), "{diags:?}");
    }

    /// The issue's repro: A, B and C render styles, D is the only empty block.
    #[test]
    fn flags_only_the_genuinely_empty_block_of_the_repro() {
        let src = r#"@import "tailwindcss";

body {
  @apply bg-background text-foreground;
}

.card {
  & .title {
    color: red;
  }
}

.panel {
  @media (min-width: 40rem) {
    color: blue;
  }
}

.ghost {
}

.solid {
  color: green;
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].line, 19);
    }
}
