//! Flag `@font-face` at-rules whose body omits `font-display`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["at_rule"] prefilter = ["@font-face"] => |node, source, ctx, diagnostics|
    let mut c = node.walk();
    let kids: Vec<_> = node.children(&mut c).collect();
    let Some(kw) = kids.iter().find(|n| n.kind() == "at_keyword") else { return };
    if !kw.utf8_text(source).is_ok_and(|t| t.eq_ignore_ascii_case("@font-face")) { return; }

    let Some(block) = kids.iter().find(|n| n.kind() == "block") else { return };
    let mut bc = block.walk();
    let has_font_display = block.children(&mut bc).any(|decl| {
        if decl.kind() != "declaration" { return false; }
        let mut dc = decl.walk();
        decl.children(&mut dc).any(|n| {
            n.kind() == "property_name"
                && n.utf8_text(source).is_ok_and(|t| t.eq_ignore_ascii_case("font-display"))
        })
    });
    if has_font_display { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`@font-face` block is missing `font-display: swap` — text will be invisible while the font loads.".into(),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.css")
    }

    #[test]
    fn flags_font_face_missing_font_display() {
        let css = "@font-face { font-family: 'Foo'; src: url('foo.woff2'); }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_font_face_with_display_swap() {
        let css = "@font-face { font-family: 'Foo'; src: url('foo.woff2'); font-display: swap; }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn flags_multiple_blocks_independently() {
        let css =
            "@font-face { font-family: a; } @font-face { font-family: b; font-display: swap; }";
        assert_eq!(run(css).len(), 1);
    }
}
