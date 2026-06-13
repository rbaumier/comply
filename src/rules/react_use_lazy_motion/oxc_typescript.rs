//! OxcCheck backend for react-use-lazy-motion.
//!
//! Flags `import { motion } from 'framer-motion'` (or `motion/react`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const FRAMER_SOURCES: &[&str] = &["framer-motion", "motion/react"];

/// Package names of the framer-motion monorepo itself. When the scanned file's
/// nearest `package.json` declares one of these as its own `name`, the file is
/// part of the library that provides `motion`/`LazyMotion` — its source and
/// tests import `motion` directly to implement and verify it, so the
/// `LazyMotion` + `m` advice (a consumer-side bundle-size optimization) does not
/// apply.
const FRAMER_SELF_NAMES: &[&str] = &["framer-motion", "motion", "motion-dom", "motion-utils"];

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

        // The framer-motion library's own packages import `motion` directly to
        // implement and test it; the consumer-side `LazyMotion` + `m` advice is
        // circular there.
        if let Some(pkg) = ctx.project.nearest_package_json(ctx.path)
            && FRAMER_SELF_NAMES.iter().any(|name| pkg.is_self_name(name))
        {
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
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

    // ── Regression for #1845: framer-motion's own packages ─────────────────
    // Run the rule against a file living under a real `package.json` so
    // `nearest_package_json` resolves the importing package's own name.
    fn run_in_pkg(pkg_name: &str, file: &str, source: &str) -> Vec<Diagnostic> {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            format!(r#"{{"name":"{pkg_name}"}}"#),
        )
        .unwrap();
        let path = dir.path().join(file);
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &path,
            crate::project::default_static_project_ctx(),
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    #[test]
    fn allows_motion_in_framer_motion_package_itself() {
        assert!(
            run_in_pkg(
                "framer-motion",
                "src/motion/__tests__/component.test.tsx",
                r#"import { motion } from "framer-motion";"#,
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_motion_react_in_motion_package_itself() {
        assert!(
            run_in_pkg(
                "motion",
                "src/render/__tests__/index.test.tsx",
                r#"import { motion } from "motion/react";"#,
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_motion_in_consumer_package() {
        assert_eq!(
            run_in_pkg("my-app", "src/App.tsx", r#"import { motion } from "framer-motion";"#).len(),
            1
        );
    }
}
