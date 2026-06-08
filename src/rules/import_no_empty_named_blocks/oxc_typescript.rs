//! OxcCheck backend — forbid `import { } from '...'`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else { return };

        // Check: has specifiers list but it's empty (i.e. `import { } from 'x'`).
        // A bare `import 'x'` has None specifiers. A default import like
        // `import foo from 'x'` would have ImportDefaultSpecifier.
        // We need to detect the case where there are braces but nothing inside.
        if let Some(specifiers) = &import.specifiers {
            // The source text must contain braces for this to be an empty named block.
            // Bare `import 'x'` has empty specifiers but no braces.
            let start = import.span.start as usize;
            let end = import.span.end as usize;
            if end > ctx.source.len() {
                return;
            }
            let text = &ctx.source[start..end];
            if let Some(open) = text.find('{')
                && let Some(close_rel) = text[open..].find('}') {
                    let between = &text[open + 1..open + close_rel];
                    if between.trim().is_empty() && specifiers.is_empty() {
                        let (line, column) = byte_offset_to_line_col(ctx.source, start);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Unexpected empty named import block.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_empty_braces() {
        let d = run_on("import { } from 'foo';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("empty named import"));
    }


    #[test]
    fn flags_empty_braces_no_space() {
        let d = run_on("import {} from 'foo';");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_named_imports() {
        assert!(run_on("import { foo } from 'bar';").is_empty());
    }
}
