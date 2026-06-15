//! tailwind-no-restricted-classes oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["className", "class"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else {
            return;
        };
        let oxc_ast::ast::JSXAttributeName::Identifier(ident) = &attr.name else {
            return;
        };
        let name = ident.name.as_str();
        if name != "className" && name != "class" {
            return;
        }
        let blocklist = ctx.config.string_list(super::META.id, "classes", ctx.lang);
        if blocklist.is_empty() {
            return;
        }
        let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
            return;
        };
        for token in lit.value.as_str().split_whitespace() {
            // Strip the variant prefix (`hover:`, `md:`, …) for matching.
            let class = token.rsplit(':').next().unwrap_or(token);
            if let Some(blocked) = blocklist.iter().find(|b| b.as_str() == class) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, attr.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Class `{blocked}` is on the project blocklist — use the \
                         design-system equivalent or remove."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
            }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    /// Run under the default config — no `classes` configured.
    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    /// Build a config that sets `classes = [...]` for the rule, then run the
    /// OXC check against it so we exercise the real config-reading path.
    fn run_with_classes(src: &str, classes: &[&str]) -> Vec<Diagnostic> {
        let tmp = TempDir::new().expect("tempdir");
        let classes_toml = classes
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(
            tmp.path().join("comply.toml"),
            format!("[rules.tailwind-no-restricted-classes]\nclasses = [{classes_toml}]\n"),
        )
        .expect("write cfg");
        let cfg = Config::load_from(tmp.path()).expect("load cfg");

        let path = Path::new("t.tsx");
        let source_type = crate::oxc_helpers::source_type_for_path(path);
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, src, source_type).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx {
            path,
            path_arc: Arc::from(path),
            source: src,
            config: &cfg,
            project: crate::project::default_static_project_ctx(),
            file: crate::rules::file_ctx::default_static_file_ctx(),
            lang: crate::files::Language::Tsx,
        };

        let mut diagnostics = Vec::new();
        let kinds = Check.interested_kinds();
        for node in semantic.nodes().iter() {
            if kinds.contains(&node.kind().ty()) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    #[test]
    fn no_op_without_configured_blocklist() {
        // bg-white / text-white / text-black are standard Tailwind utilities;
        // under the default (empty) blocklist the rule must stay silent.
        assert!(run(r#"const x = <div className="bg-white dark:bg-zinc-900" />;"#).is_empty());
        assert!(run(r#"const x = <div className="text-white" />;"#).is_empty());
        assert!(run(r#"const x = <div className="text-black mt-4" />;"#).is_empty());
        assert!(run(r#"const x = <div className="space-x-px" />;"#).is_empty());
    }

    #[test]
    fn flags_configured_class() {
        let src = r#"const x = <div className="text-black mt-4" />;"#;
        assert_eq!(run_with_classes(src, &["text-black"]).len(), 1);
    }

    #[test]
    fn flags_configured_class_through_variant_prefix() {
        let src = r#"const x = <div className="hover:bg-white" />;"#;
        assert_eq!(run_with_classes(src, &["bg-white"]).len(), 1);
    }

    #[test]
    fn ignores_unconfigured_class_when_blocklist_set() {
        let src = r#"const x = <div className="text-red-500 mt-4" />;"#;
        assert!(run_with_classes(src, &["text-black"]).is_empty());
    }
}
