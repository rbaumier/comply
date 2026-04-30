use crate::diagnostic::{Diagnostic, Severity};

/// Known i18n libraries declared in `package.json`. The rule only fires when
/// the project actually has an i18n library wired up — flagging hardcoded
/// strings in a deliberately monolingual app is pure noise.
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

fn project_uses_i18n(ctx: &crate::rules::backend::CheckCtx) -> bool {
    let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
        return false;
    };
    I18N_DEPS.iter().any(|dep| pkg.has_dep_or_engine(dep))
}

crate::ast_check! { on ["jsx_text"] => |node, source, ctx, diagnostics|
    if !project_uses_i18n(ctx) { return; }
    let text = node.utf8_text(source).unwrap_or("").trim();
    if text.is_empty() || !text.contains(' ') || text.len() <= 2 { return; }
    if text.chars().all(|c| c.is_ascii_digit() || c.is_ascii_punctuation()) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Hardcoded string \"{text}\" in JSX — wrap with `t()`."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::config::Config;
    use crate::diagnostic::Diagnostic;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::test_helpers::run_tsx;
    use std::fs;
    use tempfile::TempDir;

    fn run(s: &str) -> Vec<Diagnostic> {
        run_tsx(s, &Check)
    }

    fn run_with_pkg(pkg_json: &str, source: &str) -> Vec<Diagnostic> {
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
        crate::rules::test_helpers::run_tsx_with_project_file_and_path(
            source,
            &Check,
            &project,
            &crate::rules::file_ctx::FileCtx::default(),
            canon.to_str().unwrap(),
        )
    }

    // Default helper: no project context -> rule skips (no i18n detected).
    #[test]
    fn skips_without_project_ctx() {
        assert!(run("<div>Hello World</div>").is_empty());
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
    fn flags_with_react_intl() {
        let pkg = r#"{"dependencies":{"react-intl":"^6"}}"#;
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

    #[test]
    fn flags_with_dev_dependency_i18n() {
        let pkg = r#"{"devDependencies":{"@lingui/react":"^4"}}"#;
        let d = run_with_pkg(pkg, "<div>Hello World</div>");
        assert_eq!(d.len(), 1);
    }
}
