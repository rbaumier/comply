//! i18n-no-hardcoded-string-in-jsx oxc backend — flag hardcoded text in JSX
//! when the project uses an i18n library.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const I18N_DEPS: &[&str] = &[
    "react-i18next",
    "i18next",
    "next-i18next",
    "react-intl",
    "@formatjs/intl",
    "@formatjs/react-intl",
    "vue-i18n",
    "@nuxtjs/i18n",
    "@angular/localize",
    "svelte-i18n",
    "lingui",
    "@lingui/core",
    "@lingui/react",
];

fn project_uses_i18n(ctx: &CheckCtx) -> bool {
    let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
        return false;
    };
    I18N_DEPS.iter().any(|dep| pkg.has_dep_or_engine(dep))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !project_uses_i18n(ctx) {
            return Vec::new();
        }

        use oxc_ast::AstKind;

        let mut diagnostics = Vec::new();

        // Walk all nodes looking for JSXText.
        for node in semantic.nodes().iter() {
            if let AstKind::JSXText(text) = node.kind() {
                let value = text.value.as_str().trim();
                if value.is_empty() || !value.contains(' ') || value.len() <= 2 {
                    continue;
                }
                if value
                    .chars()
                    .all(|c| c.is_ascii_digit() || c.is_ascii_punctuation())
                {
                    continue;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, text.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("Hardcoded string \"{value}\" in JSX — wrap with `t()`."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::config::Config;
    use crate::diagnostic::Diagnostic;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use std::fs;
    use tempfile::TempDir;

    fn run_with_pkg(pkg_json: &str, source: &str) -> Vec<Diagnostic> {
        use crate::rules::backend::{CheckCtx, OxcCheck};
        use oxc_allocator::Allocator;
        use oxc_parser::Parser as OxcParser;
        use oxc_semantic::SemanticBuilder;
        use oxc_span::SourceType;
        use std::path::Path;

        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join("src/Page.tsx");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, source).unwrap();
        let lang = Language::from_path(&file_path).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: lang,
        };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = fs::canonicalize(&file_path).unwrap();

        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::tsx()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&canon, source, &project);
        Check.run_on_semantic(&semantic, &ctx)
    }

    #[test]
    fn skips_without_project_ctx() {
        assert!(
            crate::rules::test_helpers::run_oxc_tsx("<div>Hello World</div>", &Check).is_empty()
        );
    }

    #[test]
    fn skips_when_project_has_no_i18n_lib() {
        let pkg = r#"{"dependencies":{"react":"^19"}}"#;
        let d = run_with_pkg(pkg, "<div>Hello World</div>");
        assert!(d.is_empty(), "monolingual app should not be flagged: {d:?}");
    }

    #[test]
    fn flags_text_content_with_react_i18next() {
        let pkg = r#"{"dependencies":{"react":"^19","react-i18next":"^14"}}"#;
        let d = run_with_pkg(pkg, "<div>Hello World</div>");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_paragraph_with_i18next() {
        let pkg = r#"{"dependencies":{"i18next":"^23"}}"#;
        let d = run_with_pkg(pkg, "<p>Submit your application</p>");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_translation_call_even_with_i18n_lib() {
        let pkg = r#"{"dependencies":{"react-i18next":"^14"}}"#;
        let d = run_with_pkg(pkg, "<div>{t('home.greeting')}</div>");
        assert!(d.is_empty());
    }

    #[test]
    fn allows_whitespace_only() {
        let pkg = r#"{"dependencies":{"react-i18next":"^14"}}"#;
        assert!(run_with_pkg(pkg, "<div> </div>").is_empty());
    }

    #[test]
    fn allows_single_char() {
        let pkg = r#"{"dependencies":{"react-i18next":"^14"}}"#;
        assert!(run_with_pkg(pkg, "<span>:</span>").is_empty());
    }

    use crate::rules::test_helpers::run_oxc_tsx;


    fn run(s: &str) -> Vec<Diagnostic> {
        run_oxc_tsx(s, &Check)
    }
}
