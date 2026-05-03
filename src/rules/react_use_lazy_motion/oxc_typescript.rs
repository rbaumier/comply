//! OxcCheck backend for react-use-lazy-motion.
//!
//! Flags `import { motion } from 'framer-motion'` (or `motion/react`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const FRAMER_SOURCES: &[&str] = &["framer-motion", "motion/react"];

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
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };

        let source_value = import.source.value.as_str();
        if !FRAMER_SOURCES.contains(&source_value) {
            return;
        }

        // Check if any specifier imports `motion`
        let Some(specifiers) = &import.specifiers else {
            return;
        };
        let has_motion = specifiers.iter().any(|spec| {
            if let oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(named) = spec {
                return named.imported.name().as_str() == "motion";
            }
            false
        });
        if !has_motion {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Import `m` with `LazyMotion` instead of `motion` — saves ~30kB in bundle size."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_named_motion_from_framer_motion() {
        assert_eq!(run(r#"import { motion } from 'framer-motion';"#).len(), 1);
    }

    #[test]
    fn flags_named_motion_from_motion_react() {
        assert_eq!(run(r#"import { motion } from 'motion/react';"#).len(), 1);
    }

    #[test]
    fn flags_named_motion_with_other_specifiers() {
        assert_eq!(
            run(r#"import { AnimatePresence, motion } from 'framer-motion';"#).len(),
            1
        );
    }

    #[test]
    fn allows_lazy_motion_import() {
        assert!(run(r#"import { LazyMotion, m } from 'framer-motion';"#).is_empty());
    }

    #[test]
    fn allows_animate_presence_only() {
        assert!(run(r#"import { AnimatePresence } from 'framer-motion';"#).is_empty());
    }

    #[test]
    fn allows_motion_from_other_package() {
        assert!(run(r#"import { motion } from 'some-other-lib';"#).is_empty());
    }
}
