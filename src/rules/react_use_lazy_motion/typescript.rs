//! Flags `import { motion } from 'framer-motion'` (or `motion/react`) —
//! pulls the full animation engine. Recommend `LazyMotion` + `m` instead.

use crate::diagnostic::{Diagnostic, Severity};

const FRAMER_SOURCES: &[&str] = &["framer-motion", "motion/react"];

fn import_source<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let src = node.child_by_field_name("source")?;
    let raw = src.utf8_text(source).ok()?;
    Some(raw.trim_matches(|c| c == '"' || c == '\''))
}

fn imports_motion_specifier(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "import_clause" {
            continue;
        }
        let mut sub = child.walk();
        for c in child.children(&mut sub) {
            if c.kind() != "named_imports" {
                continue;
            }
            let mut named = c.walk();
            for spec in c.children(&mut named) {
                if spec.kind() != "import_specifier" {
                    continue;
                }
                let Some(name_node) = spec.child_by_field_name("name") else {
                    continue;
                };
                if name_node.utf8_text(source).ok() == Some("motion") {
                    return true;
                }
            }
        }
    }
    false
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let Some(import_path) = import_source(node, source) else { return };
    if !FRAMER_SOURCES.contains(&import_path) {
        return;
    }
    if !imports_motion_specifier(node, source) {
        return;
    }
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: "Import `m` with `LazyMotion` instead of `motion` — saves ~30kB in bundle size.".into(),
        severity: Severity::Warning,
        span: None,
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
}
