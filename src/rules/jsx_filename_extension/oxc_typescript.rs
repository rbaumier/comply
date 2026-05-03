use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else { return };

        let ext = ctx
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if ext != "js" && ext != "ts" {
            return;
        }

        // Only report on the first JSX element found — check if we already emitted.
        if diagnostics.iter().any(|d| d.rule_id.as_ref() == super::META.id) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, opening.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "JSX found in `.{ext}` file \u{2014} rename the file to `.{ext}x` or move the JSX out."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_with_path(source: &str, fake_path: &str) -> Vec<Diagnostic> {
        // Parse as TSX so the parser accepts JSX syntax, but present the
        // fake path so the rule sees the `.ts` / `.js` extension.
        use crate::rules::backend::{CheckCtx, OxcCheck as _};
        use oxc_parser::Parser as OxcParser;
        use oxc_semantic::SemanticBuilder;
        use oxc_allocator::Allocator;
        use oxc_span::SourceType;
        use std::path::Path;
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::tsx()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test(Path::new(fake_path), source);
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            Check.run(node, &ctx, &semantic, &mut diagnostics);
        }
        diagnostics
    }

    #[test]
    fn flags_jsx_in_js_file() {
        assert_eq!(run_with_path("const x = <div />;", "a.js").len(), 1);
    }

    #[test]
    fn flags_jsx_in_ts_file() {
        assert_eq!(run_with_path("const x = <div>hi</div>;", "a.ts").len(), 1);
    }

    #[test]
    fn allows_jsx_in_tsx_file() {
        assert!(run_with_path("const x = <div />;", "a.tsx").is_empty());
    }

    #[test]
    fn allows_jsx_in_jsx_file() {
        assert!(run_with_path("const x = <div />;", "a.jsx").is_empty());
    }

    #[test]
    fn allows_plain_ts_without_jsx() {
        assert!(run_with_path("const x = 1;", "a.ts").is_empty());
    }

    #[test]
    fn reports_only_first_jsx_occurrence() {
        assert_eq!(
            run_with_path("const x = <div />; const y = <span />;", "a.ts").len(),
            1
        );
    }
}
