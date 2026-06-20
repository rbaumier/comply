//! OXC backend for react-no-barrel-import-known-libs.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const BARREL_LIBS: &[&str] = &["@mui/material", "@mui/icons-material", "lodash", "date-fns"];

const TREE_SHAKEABLE_ALLOWLIST: &[&str] = &[
    "lucide-react",
    "@heroicons/react/*",
    "@phosphor-icons/react",
    "react-icons/*",
];

fn matches_allowlist(source: &str) -> bool {
    TREE_SHAKEABLE_ALLOWLIST
        .iter()
        .any(|pat| match pat.strip_suffix('*') {
            Some(prefix) => source.starts_with(prefix),
            None => source == *pat,
        })
}

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

        // Must have named specifiers (not just default/namespace)
        let Some(specifiers) = &import.specifiers else {
            return;
        };
        let has_named = specifiers.iter().any(|s| {
            matches!(s, oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(_))
        });
        if !has_named {
            return;
        }

        let import_path = import.source.value.as_str();
        if matches_allowlist(import_path) {
            return;
        }
        if !BARREL_LIBS.contains(&import_path) {
            return;
        }
        // A package importing its own published name (e.g. a file in the
        // `date-fns` repo doing `import { format } from "date-fns"`) resolves
        // to its own source — its examples/tests are the canonical reference
        // for how consumers import it, and it cannot deep-import itself away.
        // Only consumer barrel imports are actionable.
        if ctx
            .project
            .nearest_package_json(ctx.path)
            .is_some_and(|pkg| pkg.is_self_name(import_path))
        {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.source.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Named import from `{import_path}` pulls the entire barrel — \
                 import from a subpath (e.g. `{import_path}/<name>`) for \
                 tree-shaking."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    //! Regression tests for the self-reference exemption (issue #4988): a
    //! package importing its own published barrel name must not be flagged.

    use super::Check;
    use crate::config::Config;
    use crate::diagnostic::Diagnostic;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::backend::{CheckCtx, OxcCheck};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    fn run_with_pkg_at_path(pkg_json: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join(rel_path);
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

        let source_type = match lang {
            Language::Tsx => SourceType::tsx(),
            Language::JavaScript => SourceType::cjs(),
            _ => SourceType::ts(),
        };
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, source_type).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let file_ctx = crate::rules::file_ctx::FileCtx::build(&canon, source, lang, &project);
        let ctx = CheckCtx::for_test_full(&canon, source, &project, &file_ctx);

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
    fn allows_self_barrel_import_in_own_package() {
        // date-fns's own example importing the barrel by its published name.
        let pkg = r#"{"name": "date-fns"}"#;
        let src = r#"import { format } from "date-fns";"#;
        let d = run_with_pkg_at_path(pkg, "pkgs/core/examples/node-esm/example.js", src);
        assert!(d.is_empty());
    }

    #[test]
    fn flags_consumer_barrel_import() {
        // A different package consuming date-fns via its barrel is still flagged.
        let pkg = r#"{"name": "my-app", "dependencies": {"date-fns": "^3"}}"#;
        let src = r#"import { format } from "date-fns";"#;
        let d = run_with_pkg_at_path(pkg, "src/index.ts", src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_barrel_import_without_package_json_name() {
        // No `name` field → not a self-reference → consumer import still flagged.
        let pkg = r#"{"dependencies": {"lodash": "^4"}}"#;
        let src = r#"import { debounce } from "lodash";"#;
        let d = run_with_pkg_at_path(pkg, "src/index.ts", src);
        assert_eq!(d.len(), 1);
    }
}
