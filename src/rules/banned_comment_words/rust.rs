//! banned-comment-words — Rust backend.
//!
//! Walks `line_comment` and `block_comment` nodes and flags inline `//` and
//! `/* */` comments whose body contains a dismissive filler word at a word
//! boundary. The diagnostic is anchored on the word, so a `/* */` block that
//! runs over several lines points at the line the word is on.
//!
//! Four kinds of comment are out of scope: doc comments (`///`, `//!`,
//! `/** */`, `/*! */`), safety comments (`// SAFETY: …`), any comment in a
//! test context, and a comment block already over the word budget.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::comment_blocks::{self, RawComment};
use crate::rules::rust_helpers::{is_safety_marker, is_test_code};
use std::sync::Arc;

pub struct Check;

/// One banned word found in a comment the per-comment guards let through.
struct Match {
    word: &'static str,
    line: usize,
    column: usize,
    start_byte: usize,
}

/// What the walk collects: every comment, so the block budget can be measured
/// over the runs a reader sees, and the matches inside them.
#[derive(Default)]
struct State {
    comments: Vec<RawComment>,
    matches: Vec<Match>,
}

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["line_comment", "block_comment"])
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
        let collected = state.unwrap().downcast_mut::<State>().unwrap();
        if let Some(comment) = comment_blocks::from_tree_sitter(&node, ctx.source) {
            collected.comments.push(comment);
        }
        let source = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source) else {
            return;
        };
        // Doc comments (`///`, `//!`, `/** */`, `/*! */`) are deliberate API prose
        // rendered by rustdoc, where words like `just`/`only`/`simply` are legitimate
        // precision qualifiers, not dismissive filler. Mirror `comment-prose-quality`,
        // which restricts this class of prose check to inline comments. Only inline
        // `//` and `/* */` comments are checked.
        if comment_blocks::is_doc_comment(text) {
            return;
        }
        let Some((word, offset)) = super::find_banned_word(text) else {
            return;
        };
        // `SAFETY:` opens a documented precondition. `rust-undocumented-unsafe`
        // requires that comment; this rule's remediation is to delete it. Both rules
        // read the marker through `is_safety_marker`, so they cannot disagree.
        if is_safety_marker(text.trim_start()) {
            return;
        }
        // A comment in a test characterises the fixture the test feeds in. "A string
        // that's clearly broken" states how malformed an input is; no production
        // complexity sits behind it.
        if is_test_code(node, source, ctx) {
            return;
        }
        let (line, column) = word_position(node, text, offset);
        collected.matches.push(Match {
            word,
            line,
            column,
            start_byte: node.start_byte() + offset,
        });
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let collected = *state.unwrap().downcast::<State>().unwrap();
        let budget = super::explanation_budget(ctx);
        let explained = super::explained_rows(collected.comments, ctx.source, budget);
        for found in collected
            .matches
            .iter()
            .filter(|found| !explained.contains(&found.line))
        {
            let word = found.word;
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: found.line,
                column: found.column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Comment uses `{word}` — dismissive filler that hides complexity. \
                     Either explain the actual subtlety or delete the comment if the \
                     line is genuinely self-explanatory."
                ),
                severity: Severity::Error,
                span: Some((found.start_byte, word.len())),
            });
        }
    }
}

/// The file position of byte `offset` into `text`, the source text of `node`,
/// as a 1-based `(line, column)` whose column counts bytes — the convention
/// [`Diagnostic::at_node`] establishes for tree-sitter rules.
///
/// tree-sitter reports a `/* … */` comment as one node however tall it is, so
/// the node's own position locates only the opening delimiter.
fn word_position(node: tree_sitter::Node<'_>, text: &str, offset: usize) -> (usize, usize) {
    let start = node.start_position();
    let prefix = &text[..offset];
    let column = match prefix.rfind('\n') {
        Some(last_newline) => offset - last_newline,
        None => start.column + offset + 1,
    };
    (start.row + 1 + prefix.matches('\n').count(), column)
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_line_comment() {
        assert_eq!(run("// This simply works\nfn f() {}").len(), 1);
    }

    #[test]
    fn flags_block_comment() {
        assert_eq!(run("/* obviously fine */\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_simplify() {
        assert!(run("// We simplify the input\nfn f() {}").is_empty());
    }

    #[test]
    fn ignores_banned_word_in_code() {
        assert!(run("fn obviously_works() {}").is_empty());
    }

    #[test]
    fn flags_crucially() {
        assert_eq!(run("// crucially this runs first\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_actually_and_inherently() {
        // Both words are excluded as too false-positive-prone in code.
        assert!(run("// actually safe because inherently single-threaded\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_banned_word_in_doc_comment_issue_6462() {
        assert!(run("/// just the word \"deprecated\"\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_banned_word_in_inner_doc_comment() {
        assert!(run("//! just a module-level doc\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_banned_word_in_block_doc_comment() {
        assert!(run("/** obviously fine */\nfn f() {}").is_empty());
    }

    #[test]
    fn flags_banned_word_in_inline_comment_still() {
        assert_eq!(run("// just do it\nfn f() {}").len(), 1);
        assert_eq!(run("/* simply */\nfn f() {}").len(), 1);
    }

    /// The `starship/starship` shape reported in #8366, reduced: a block comment
    /// holding its banned word four lines below the `/*`, a `SAFETY:`
    /// justification, a production comment, and a `#[cfg(test)]` module.
    const STARSHIP_SHAPE: &str = r#"/* Two-phase init: the first phase hands the shell a command.
That command sources a longer script.


In future this may be changed to just evaluate the initscript.

*/

pub fn dispatch(name: &str) -> Option<&str> {
    match name {
        custom if custom.starts_with("custom.") => {
            // SAFETY: We just checked that the module starts with "custom."
            Some(custom.strip_prefix("custom.").unwrap())
        }
        _ => None,
    }
}

// just do it
pub fn control() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_malformed() {
        // Test a string that's clearly broken
        assert!(dispatch("]]]").is_none());
    }
}
"#;

    /// Assert every diagnostic starts the word its own message names, reading the
    /// reported line back out of `src` rather than restating the rule's
    /// arithmetic.
    fn assert_anchored_on_its_word(src: &str, diags: &[Diagnostic]) {
        for d in diags {
            let word = d
                .message
                .split('`')
                .nth(1)
                .expect("the message quotes the word it found");
            let line = src.lines().nth(d.line - 1).expect("reported line exists");
            assert_eq!(&line[d.column - 1..d.column - 1 + word.len()], word);
            let (offset, len) = d.span.expect("anchored on the word's byte range");
            assert_eq!(&src[offset..offset + len], word);
        }
    }

    #[test]
    fn anchors_block_comment_on_the_word_not_the_opening_line_issue_8366() {
        // The block comment opens on line 1 and holds `just` on line 5. The
        // `SAFETY:` and `#[cfg(test)]` comments are out of scope.
        let diags = run(STARSHIP_SHAPE);
        assert_eq!(diags.iter().map(|d| d.line).collect::<Vec<_>>(), vec![5, 19]);
        assert_anchored_on_its_word(STARSHIP_SHAPE, &diags);
    }

    #[test]
    fn anchors_on_the_last_line_of_a_block_comment() {
        let src = "/* opening line, all clear here\n   second line\n   and here it is just wrong */\nfn f() {}\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
        assert_anchored_on_its_word(src, &diags);
    }

    #[test]
    fn anchors_on_the_word_in_an_indented_line_comment() {
        // The node starts mid-line, so the column has to carry the indentation
        // as well as the offset of the word inside the comment.
        let src = "fn f() {\n    // just do it\n}\n";
        let diags = run(src);
        assert_eq!((diags[0].line, diags[0].column), (2, 8));
        assert_anchored_on_its_word(src, &diags);
    }

    #[test]
    fn anchors_on_the_word_in_an_indented_block_comment() {
        // Indented node start and a multi-line body at once: the column comes
        // from the last newline, not from the node.
        let src = "fn f() {\n    /* opening line\n       just wrong */\n}\n";
        let diags = run(src);
        assert_eq!((diags[0].line, diags[0].column), (3, 8));
        assert_anchored_on_its_word(src, &diags);
    }

    #[test]
    fn anchors_by_byte_column_on_a_multibyte_line() {
        // `naïve` and the em dash are multibyte, so a byte column and a character
        // column differ here. tree-sitter rules report bytes, which is what
        // `Diagnostic::at_node` reports and what `assert_anchored_on_its_word`
        // slices by.
        let src = "// naïve — just do it\nfn f() {}\n";
        let diags = run(src);
        assert_eq!((diags[0].line, diags[0].column), (1, 15));
        assert_anchored_on_its_word(src, &diags);
    }

    #[test]
    fn reports_one_diagnostic_per_comment() {
        assert_eq!(run("// just simply do it\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_safety_comment_issue_8366() {
        // `rust-undocumented-unsafe` demands a `// SAFETY:` justification, and
        // "we just checked" is the temporal sense of `just`. Flagging it leaves
        // the author no wording both rules accept.
        let src = "fn f(s: &str) {\n    // SAFETY: We just checked that the module starts with \"custom.\"\n    let _ = s;\n}\n";
        assert!(run(src).is_empty());
        assert!(run("/* SAFETY: the pointer is just the one we allocated */\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_every_spelling_of_the_safety_marker() {
        // The accepted spellings are `is_safety_marker`'s, which is how
        // `rust-undocumented-unsafe` reads a comment. A casing or punctuation
        // test of this rule's own would put the two rules back in disagreement.
        assert!(run("// safety: just do it\nfn f() {}").is_empty());
        assert!(run("// Safety we just assume it\nfn f() {}").is_empty());
    }

    #[test]
    fn flags_comment_that_only_mentions_safety_in_prose() {
        // The marker has to open the comment. A comment that talks about safety
        // partway through is ordinary prose and stays in scope.
        assert_eq!(run("// we just assume safety here\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_banned_word_in_test_context_issue_8366() {
        // The comment characterises the fixture — how malformed the input is —
        // rather than papering over production complexity.
        let cfg_test = "#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {\n        // Test a string that's clearly broken\n    }\n}\n";
        assert!(run(cfg_test).is_empty());
        let test_fn = "#[test]\nfn t() {\n    // this simply works\n}\n";
        assert!(run(test_fn).is_empty());
        let block = "#[test]\nfn t() {\n    /* this\n       simply works */\n}\n";
        assert!(run(block).is_empty());
    }

    #[test]
    fn allows_banned_word_in_a_block_over_the_word_budget_issue_8184() {
        // Past the `comment-max-block-words` budget the block has spent what an
        // explanation costs, and that rule reports it for the length. Both
        // verdicts on one block would leave no wording either accepts.
        let src = "\
// The reader needs the whole story here, so this note names the compiler
// version, the shorter form that fails to build, what the checker infers
// instead, why that inference is wrong for this call, and it ends on the
// upstream issue, because we could just write the shorter form otherwise.
fn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_banned_word_in_a_block_under_the_word_budget() {
        // The gate is the block's length, so a short note stays in scope.
        let src = "// we could just write the shorter form here\nfn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_banned_word_outside_the_test_module_of_the_same_file() {
        // The guard is scoped to the test item, not to the file: production code
        // sitting next to a `#[cfg(test)]` module is still checked.
        let src = "// just do it\npub fn control() {}\n\n#[cfg(test)]\nmod tests {\n    // clearly broken\n}\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert_eq!((diags[0].line, diags[0].column), (1, 4));
    }
}
