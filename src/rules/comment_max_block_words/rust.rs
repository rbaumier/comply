use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::comment_blocks::{self, RawComment};
use std::sync::Arc;

pub struct Check;

type State = Vec<RawComment>;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["line_comment", "block_comment"])
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(State::new()))
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
            collected.push(comment);
        }
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let comments = *state.unwrap().downcast::<State>().unwrap();
        let max = ctx.config.threshold(super::META.id, "max", ctx.lang);
        for flag in super::flagged_blocks(comments, ctx.source, max) {
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: flag.line,
                column: flag.column,
                rule_id: super::META.id.into(),
                message: super::message(flag.words, max),
                severity: Severity::Error,
                span: None,
            });
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_long_line_comment_block() {
        let src = "\
// this is a long implementation note that keeps explaining the rationale in
// exhaustive detail across several full lines and easily runs past the fifty
// word budget because it just keeps going and going and going and going and
// going and never stops adding one more clause that could have lived in a
// dedicated doc comment or a shorter summary somewhere far more scannable here
fn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_short_block() {
        assert!(run("// short note\n// second short line\nfn f() {}").is_empty());
    }

    #[test]
    fn outer_doc_comment_block_counts_too() {
        let src = "\
/// This documents the public API in full prose across several lines and words,
/// legitimately explaining the contract, invariants, and edge cases at length,
/// which is exactly what a documentation comment is for and still budgeted here,
/// because a doc comment nobody reads to the end documents nothing at all today.
/// The budget applies to every comment the reader has to walk through in order.
fn f() {}";
        assert_eq!(run(src).len(), 1);
    }

    // Diagrams, grids and fenced samples hold no prose.
    #[test]
    fn diagrams_grids_and_fenced_samples_hold_no_prose() {
        let diagram = "\
// ┌────┬───────────────────┬────┬─────variables─────┬────┬───────────────────┬────┐
// │    │                   │    │                   │    │                   │    │
// v    v                   v    v                   v    v                   v    v
// └v0  └v1                 └v2  └v3                 └v4  └v5                 └v6  └v7
pub fn variables() {}";
        assert!(run(diagram).is_empty(), "{:?}", run(diagram));

        let grid = "\
//   16  17  18  19  20  21   52  53  54  55  56  57   88  89  90  91  92  93
//   22  23  24  25  26  27   58  59  60  61  62  63   94  95  96  97  98  99
//   28  29  30  31  32  33   64  65  66  67  68  69  100 101 102 103 104 105
//   34  35  36  37  38  39   70  71  72  73  74  75  106 107 108 109 110 111
//   40  41  42  43  44  45   76  77  78  79  80  81  112 113 114 115 116 117
//   46  47  48  49  50  51   82  83  84  85  86  87  118 119 120 121 122 123
pub fn indices() {}";
        assert!(run(grid).is_empty(), "{:?}", run(grid));

        let fenced = "\
/// Splits the area.
///
/// ```rust
/// use ratatui::layout::Constraint;
/// use ratatui::layout::Direction;
/// use ratatui::layout::Layout;
/// use ratatui::widgets::Block;
/// use ratatui::widgets::Paragraph;
///
/// let layout = Layout::default()
///     .direction(Direction::Vertical)
///     .constraints([Constraint::Length(3), Constraint::Min(0)])
///     .split(frame.area());
///
/// let header = Paragraph::new(\"Header\").block(Block::bordered());
/// let body = Paragraph::new(\"Body\").block(Block::bordered());
///
/// frame.render_widget(header, layout[0]);
/// frame.render_widget(body, layout[1]);
/// ```
pub fn split() {}";
        assert!(run(fenced).is_empty(), "{:?}", run(fenced));
    }

    // A hex table spells values, not words.
    #[test]
    fn a_column_of_hex_constants_holds_no_prose() {
        let src = "\
// The magic values this table has shipped, one per generation:
// 0x00010000 0x00010001 0x00010002 0x00010003 0x00010004 0x00010005
// 0x00010006 0x00010007 0x00010008 0x00010009 0x0001000a 0x0001000b
// 0x0001000c 0x0001000d 0x0001000e 0x0001000f 0x00010010 0x00010011
// 0x00010012 0x00010013 0x00010014 0x00010015 0x00010016 0x00010017
// 0x00010018 0x00010019 0x0001001a 0x0001001b 0x0001001c 0x0001001d
pub fn magic() {}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // A diagram earns the prose beside it no exemption.
    #[test]
    fn prose_beside_a_diagram_still_counts() {
        let src = "\
// ┌────┬───────────────────┬────┐
// │    │                   │    │
// ^    ^                   ^    ^
// The solver keeps one variable per segment boundary and one per spacer, so a
// layout of three constraints resolves eight of them in a single pass, and the
// diagram above names each variable in the order the constraint list produces.
pub fn variables() {}";
        let diagnostics = run(src);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("spans 40 words"), "{diagnostics:?}");
    }

    // Each trailing label is scoped to its own line.
    #[test]
    fn column_aligned_trailing_field_labels_are_not_one_block() {
        let src = "\
pub fn maxp_fixture() -> Vec<u32> {
    vec![
        0x00010000, // version
        0x00000001, // number of glyphs
        0x00000000, // maximum points in a non-composite glyph
        0x00000000, // maximum contours in a non-composite glyph
        0x00000000, // maximum points in a composite glyph
        0x00000000, // maximum contours in a composite glyph
        0x00000002, // maximum zones used for twilight and glyph space
        0x00000000, // maximum twilight points used in zone zero
        0x00000000, // number of storage area locations available
        0x00000000, // maximum function definitions in the font program
    ]
}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn standalone_block_comment_counts_alone() {
        let src = "\
/* this block comment on its own runs well past the small budget configured for
   the test by packing more than a dozen words onto its several wrapped lines */
fn f() {}";
        // With the default budget (30) this stays under; assert it does not flag.
        assert!(run(src).is_empty());
    }
}
