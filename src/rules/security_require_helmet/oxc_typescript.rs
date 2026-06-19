//! security-require-helmet oxc backend — Express app without `helmet()` middleware.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["express"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Only check files that import or create Express apps.
        if !ctx.source_contains("express") {
            return;
        }
        // If helmet() is registered anywhere in this file, we're fine.
        if ctx.source_contains("helmet(") {
            return;
        }
        // Published-library code (package.json declares `main`/`module`/
        // `exports`/`publishConfig`) — e.g. a framework's Express adapter that
        // calls `express()` so the consuming application can configure it — is
        // not the deployed app. The app author installs `helmet()` in their own
        // bootstrap, so the library wrapper must not be required to. A real app
        // is never a library, so genuine apps stay checked.
        if ctx
            .project
            .nearest_package_json(ctx.path)
            .is_some_and(|pkg| pkg.is_library)
        {
            return;
        }
        if !diagnostics.is_empty() {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::Identifier(callee) = &call.callee else { return };
        if callee.name.as_str() != "express" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Express app created without `helmet()` middleware — default security headers are missing.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    /// Run the check against `source` with a real `ProjectCtx` rooted at a
    /// tempdir whose `package.json` is `pkg_json` — exercises the
    /// published-library relaxation, which depends on `nearest_package_json`.
    fn run_with_pkg(pkg_json: &str, source: &str) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        use oxc_allocator::Allocator;
        use oxc_parser::Parser as OxcParser;
        use oxc_semantic::SemanticBuilder;
        use oxc_span::SourceType;

        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();
        let file_path = dir.path().join("src/server.ts");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, source).unwrap();
        let source_file = SourceFile {
            path: file_path.clone(),
            language: Language::TypeScript,
        };
        let refs = vec![&source_file];
        let config = Config::default();
        let project = ProjectCtx::load(&refs, &config);
        let canon = std::fs::canonicalize(&file_path).unwrap();

        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::ts()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test_with_project(&canon, source, &project);

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
    fn flags_bare_express_app_without_helmet() {
        let pkg = r#"{ "name": "my-app", "private": true }"#;
        let src = "const app = express();\napp.listen(3000);\n";
        assert_eq!(run_with_pkg(pkg, src).len(), 1);
    }

    #[test]
    fn allows_express_app_with_helmet() {
        let pkg = r#"{ "name": "my-app", "private": true }"#;
        let src = "const app = express();\napp.use(helmet());\n";
        assert!(run_with_pkg(pkg, src).is_empty());
    }

    // --- Issue #3258 regression: published-library framework adapters ---

    #[test]
    fn skips_express_in_published_library_with_publish_config() {
        // The user's repro from issue #3258: a NestJS-style Express adapter in a
        // published monorepo package (declares `publishConfig`) calls `express()`
        // so the consuming application can configure it. The app author installs
        // `helmet()` in their own bootstrap, so the adapter must not be flagged.
        let pkg = r#"{
            "name": "@nestjs/platform-express",
            "publishConfig": { "access": "public" }
        }"#;
        let src = "super(instance || express());\n";
        assert!(run_with_pkg(pkg, src).is_empty(), "{:?}", run_with_pkg(pkg, src));
    }

    #[test]
    fn skips_express_in_published_library_with_main_exports() {
        let pkg = r#"{
            "name": "@x/platform",
            "main": "./dist/index.js",
            "exports": { ".": "./dist/index.js" }
        }"#;
        let src = "const app = express();\n";
        assert!(run_with_pkg(pkg, src).is_empty());
    }

    #[test]
    fn flags_express_in_non_library_application_package() {
        // The same bare `express()` in an application package (no `main`/
        // `module`/`exports`/`publishConfig`) is the rule's legitimate target —
        // the deployed app must install `helmet()`, so no security false negative.
        let pkg = r#"{ "name": "my-app", "private": true }"#;
        let src = "const app = express();\n";
        assert_eq!(run_with_pkg(pkg, src).len(), 1);
    }
}
