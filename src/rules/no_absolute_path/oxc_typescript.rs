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
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let spec = import.source.value.as_str();
        if !spec.starts_with('/') {
            return;
        }
        // A `/`-leading specifier that matches a configured tsconfig
        // `compilerOptions.paths` alias (e.g. `/@/* → src/*`) is an aliased
        // import resolving into the project, not a filesystem-absolute path;
        // only genuine OS-absolute imports are flagged.
        if ctx.project.matches_tsconfig_path_alias(ctx.path, spec) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.source.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Do not import modules using an absolute path (`{spec}`)."),
            severity: Severity::Error,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_absolute_import() {
        let src = "import { foo } from '/usr/lib/utils';\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("absolute path"));
    }

    #[test]
    fn allows_relative_import() {
        let src = "import { foo } from './utils';\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_package_import() {
        let src = "import { foo } from 'lodash';\n";
        assert!(run(src).is_empty());
    }

    /// Run the rule on `source` (a `.ts` file under `src/`) with `tsconfig`
    /// (literal JSON) written at the tempdir root, so `nearest_tsconfig` resolves
    /// the on-disk `compilerOptions.paths` and alias-aware behaviour is exercised.
    fn run_with_tsconfig(source: &str, tsconfig: &str) -> Vec<Diagnostic> {
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("tsconfig.json"), tsconfig).expect("write tsconfig");
        let src_path = dir.path().join("src").join("t.ts");
        std::fs::create_dir_all(src_path.parent().expect("src parent")).expect("mkdir src");
        std::fs::write(&src_path, source).expect("write source");
        let files = [SourceFile { path: src_path.clone(), language: Language::TypeScript }];
        let refs: Vec<&SourceFile> = files.iter().collect();
        let project = ProjectCtx::for_test_with_files(&refs);
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            &src_path,
            &project,
            crate::rules::file_ctx::default_static_file_ctx(),
        )
    }

    const ALIAS_TSCONFIG: &str = r#"{ "compilerOptions": { "paths": {
        "/@/*": ["src/*"], "/#/*": ["types/*"], "/root": ["src/root"]
    } } }"#;

    #[test]
    fn allows_tsconfig_path_alias_imports() {
        let src = "import { on } from '/@/utils/domUtils';\n\
                   import type { C } from '/#/config';\n";
        assert!(run_with_tsconfig(src, ALIAS_TSCONFIG).is_empty());
    }

    #[test]
    fn allows_bare_tsconfig_path_alias_import() {
        let src = "import r from '/root';\n";
        assert!(run_with_tsconfig(src, ALIAS_TSCONFIG).is_empty());
    }

    #[test]
    fn flags_absolute_path_not_matching_alias() {
        let src = "import x from '/usr/lib/utils';\n";
        let diags = run_with_tsconfig(src, ALIAS_TSCONFIG);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("absolute path"));
    }

    #[test]
    fn flags_absolute_import_without_tsconfig_paths() {
        let src = "import y from '/absolute/thing';\n";
        let diags = run_with_tsconfig(src, r#"{ "compilerOptions": {} }"#);
        assert_eq!(diags.len(), 1);
    }
}
