//! no-logger-in-business-logic — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

/// Path fragments that mark a business-logic directory, pre-expanded with
/// both path separators so the per-node check needs no `format!` allocation.
const BUSINESS_DIR_PATTERNS: &[&str] = &[
    "/service/", "\\service\\",
    "/domain/", "\\domain\\",
    "/core/", "\\core\\",
    "/model/", "\\model\\",
    "/entity/", "\\entity\\",
];

const CONSOLE_METHODS: &[&str] = &["log", "info", "warn", "error", "debug", "trace"];

fn is_business_logic_path(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    BUSINESS_DIR_PATTERNS.iter().any(|p| path_str.contains(p))
}

/// True when the file's name marks it as dedicated logging infrastructure
/// (`logging.interceptor.ts`, `log.service.ts`, `audit-log.ts`, `logger.ts`),
/// whose sole purpose is to log — `console.*`/`logger.*` there is the file's
/// reason to exist, not a cross-cutting leak into business logic.
///
/// Matching is on whole `.`/`-`/`_`-delimited stem segments, so a segment that
/// merely *contains* a logging word (`login`, `catalog`, `blog`, `dialog`) is
/// not exempted — those remain business logic.
fn is_logging_file(path: &std::path::Path) -> bool {
    let stem = path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    stem.split(['.', '-', '_'])
        .any(|seg| matches!(seg, "log" | "logger" | "logging" | "logs"))
}

/// Return the leftmost identifier name in a (possibly chained) member expression.
fn root_identifier_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(&id.name),
        Expression::ThisExpression(_) => Some("this"),
        Expression::StaticMemberExpression(mem) => root_identifier_name(&mem.object),
        Expression::ComputedMemberExpression(mem) => root_identifier_name(&mem.object),
        _ => None,
    }
}

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // A test file inside a business-logic directory (e.g.
        // `core/__tests__/logger.test.ts`) exercises the logger to assert on
        // its behaviour — it is not production business logic, so `logger.*`
        // calls there are expected, not a leak of a cross-cutting concern.
        // A dedicated logging-infrastructure file (e.g. `logging.interceptor.ts`)
        // is the logging wrapper itself, so its `console.*`/`logger.*` calls are
        // its purpose, not a leak.
        if ctx.file.path_segments.in_test_dir || is_logging_file(ctx.path) {
            return;
        }

        if !is_business_logic_path(ctx.path) {
            return;
        }

        // Published-library packages (package.json declares `main`/`module`/
        // `exports`) ship SDK/library infrastructure, not application business
        // logic — e.g. an Azure SDK's `src/core/` holds AMQP/HTTP/auth protocol
        // handlers where `@azure/logger` telemetry is the intended mechanism.
        // The rule targets application code, so it does not apply here.
        if ctx
            .project
            .nearest_package_json(ctx.path)
            .is_some_and(|pkg| pkg.is_library)
        {
            return;
        }

        let oxc_ast::AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be a static member expression (e.g. console.log, logger.info).
        let Expression::StaticMemberExpression(mem) = &call.callee else {
            return;
        };

        let prop_text = &*mem.property.name;
        let Some(root) = root_identifier_name(&mem.object) else {
            return;
        };

        let pattern = match root {
            "console" if CONSOLE_METHODS.contains(&prop_text) => format!("console.{prop_text}"),
            "logger" => "logger.".to_string(),
            _ => return,
        };

        let (line, col) =
            byte_offset_to_line_col(semantic.source_text(), call.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column: col,
            rule_id: super::META.id.into(),
            message: format!(
                "`{pattern}` in business logic — use a `withLogging()` wrapper or domain events instead."
            ),
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::files::{Language, SourceFile};
    use crate::project::ProjectCtx;
    use crate::rules::test_helpers::run_rule_gated;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    /// Run the check on `source` placed at `rel_path` inside a temp project whose
    /// root `package.json` is `pkg_json`, so `nearest_package_json` resolves the
    /// real manifest (the static default ctx has none).
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

        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::ts()).parse();
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
    fn flags_logger_in_core() {
        let diags = run_rule_gated(&Check, "logger.info('order placed');", "src/core/order.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_console_log_in_service() {
        let diags = run_rule_gated(&Check, "console.log('creating user');", "src/service/user.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn skips_logger_test_inside_core_dir() {
        // Issue #1821: a unit test for the logger module lives inside a
        // business-logic directory and calls `logger.*` to observe behaviour.
        let src = "describe('basic logging functionality', () => {\n\
                       it('should log messages at appropriate levels', () => {\n\
                           logger.error('error message');\n\
                           logger.warn('warn message');\n\
                           logger.info('info message');\n\
                           logger.debug('debug message');\n\
                       });\n\
                   });";
        let diags = run_rule_gated(
            &Check,
            src,
            "packages/clerk-js/src/core/modules/debug/__tests__/logger.test.ts",
        );
        assert!(diags.is_empty(), "test file should not flag logger.* calls");
    }

    #[test]
    fn skips_spec_file_in_business_dir() {
        let diags = run_rule_gated(&Check, "logger.info('x');", "src/domain/order.spec.ts");
        assert!(diags.is_empty());
    }

    #[test]
    fn skips_logger_in_published_library_core() {
        // Issue #1132: an Azure SDK AMQP error-event handler under `src/core/`
        // calls `@azure/logger` — SDK telemetry, not application business logic.
        // The package is published (declares `main`/`module`/`exports`), so the
        // rule does not apply.
        let pkg = r#"{
            "name": "@azure/service-bus",
            "main": "./dist/commonjs/index.js",
            "module": "./dist/esm/index.js",
            "exports": { ".": "./dist/esm/index.js" }
        }"#;
        let src = r#"
            this._onAmqpError = (context) => {
                const senderError = context.sender && context.sender.error;
                logger.logError(senderError, "%s sender_error", this.logPrefix);
            };
        "#;
        let diags = run_with_pkg_at_path(pkg, "src/core/messageSender.ts", src);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn flags_logger_in_non_library_application_core() {
        // Negative-space guard: the same `/core/` path in an application package
        // (no `main`/`module`/`exports`) is the rule's legitimate target — the
        // smell still fires there.
        let pkg = r#"{ "name": "my-app", "private": true }"#;
        let src = "logger.info('order placed');";
        let diags = run_with_pkg_at_path(pkg, "src/core/order.ts", src);
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn skips_dedicated_logging_interceptor() {
        // Issue #3260: a NestJS interceptor whose sole purpose is request
        // logging lives under `/core/` but IS the logging infrastructure.
        let diags = run_rule_gated(
            &Check,
            "console.log('Before...');",
            "src/core/interceptors/logging.interceptor.ts",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn skips_dedicated_log_service() {
        let diags = run_rule_gated(&Check, "console.log('x');", "src/core/log.service.ts");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn skips_dedicated_audit_log_util() {
        let diags = run_rule_gated(&Check, "console.log('x');", "src/core/utils/audit-log.ts");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn skips_dedicated_logger_module() {
        let diags = run_rule_gated(&Check, "console.log('x');", "src/core/logger.ts");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn flags_login_service_not_exempted_as_logging() {
        // Over-broad-guard guard: `login` merely contains "log" — it is business
        // logic and must STILL be flagged (word-segment match, not `contains`).
        let diags = run_rule_gated(&Check, "console.log('x');", "src/core/login.service.ts");
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn flags_catalog_service_not_exempted_as_logging() {
        let diags = run_rule_gated(&Check, "console.log('x');", "src/core/catalog.service.ts");
        assert_eq!(diags.len(), 1, "{diags:?}");
    }

    #[test]
    fn flags_ordinary_business_file_with_console_log() {
        // Core preserved: a regular business-logic file still fires.
        let diags = run_rule_gated(&Check, "console.log('x');", "src/core/cats.service.ts");
        assert_eq!(diags.len(), 1, "{diags:?}");
    }
}
