//! security-require-rate-limit-auth OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_auth_path(path: &str) -> bool {
    let unquoted = path.trim_matches(|c: char| c == '"' || c == '\'' || c == '`');
    let lower = unquoted.to_ascii_lowercase();
    lower.contains("/login")
        || lower.contains("/signin")
        || lower.contains("/sign-in")
        || lower.contains("/signup")
        || lower.contains("/sign-up")
        || lower.contains("/register")
        || lower.contains("/reset")
        || lower.contains("/forgot-password")
}

fn looks_like_rate_limit(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("ratelimit")
        || lower.contains("rate_limit")
        || lower.contains("rate-limit")
        || lower.contains("ratelimiter")
        || lower.contains("throttle")
        || lower.contains("slow-down")
        || lower.contains("slowdown")
}

/// True when `source` registers a global rate-limit middleware: an
/// `app.use(<rateLimiter>)` / `router.use(<rateLimiter>)` whose argument the
/// rule recognizes as a rate limiter ([`looks_like_rate_limit`]). A global
/// limiter mounted before the auth router covers every downstream route. Used
/// both same-file (the route and the `app.use` share a file) and across the
/// package (via [`crate::project::ProjectCtx::has_global_rate_limit`], which
/// scans the indexed sources so a limiter in a separate setup file still
/// counts).
pub(crate) fn has_global_rate_limit(source: &str) -> bool {
    let lower = source.to_ascii_lowercase();
    for (i, _) in lower.match_indices(".use(") {
        // Window of up to 125 bytes after the `.use(`, snapped to the next char
        // boundary so non-ASCII source (an accented char/emoji in a nearby
        // comment) can't slice mid-codepoint and panic.
        let mut end = (i + 125).min(lower.len());
        while end < lower.len() && !lower.is_char_boundary(end) {
            end += 1;
        }
        let window = &lower[i..end];
        if looks_like_rate_limit(window) {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Check callee is a member expression like app.post, router.get, etc.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method = member.property.name.as_str();
        if !matches!(method, "post" | "get" | "put" | "patch" | "all") {
            return;
        }

        // MSW request mocks (`http.post("*/api/...", handler)`) are in-process
        // test doubles, not a real network surface — there is nothing to
        // rate-limit. MSW's callee object is `http`.
        if let Expression::Identifier(obj) = &member.object
            && obj.name.as_str() == "http"
        {
            return;
        }

        // First argument must be a string literal with an auth path.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let path_text = match first_arg {
            Argument::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        // A leading `*` is an MSW URL wildcard, never valid Elysia route syntax.
        if path_text.starts_with('*') {
            return;
        }
        if !is_auth_path(path_text) {
            return;
        }

        // Check remaining args for rate-limit middleware.
        for arg in call.arguments.iter().skip(1) {
            let text = &ctx.source[arg.span().start as usize..arg.span().end as usize];
            if looks_like_rate_limit(text) {
                return;
            }
        }

        // Check for a global rate-limit middleware — first same-file, then
        // across the package. A limiter registered with `app.use(<rateLimiter>)`
        // before the auth router covers every downstream route; in larger apps
        // (e.g. Directus) that registration lives in a separate setup file from
        // the route definitions, so we also consult the package-wide scan.
        if has_global_rate_limit(ctx.source) || ctx.project.has_global_rate_limit(ctx.path) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Auth route \"{path_text}\" has no rate-limit middleware — attackers can brute-force credentials."
            ),
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

    fn run_tsx(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_auth_route_without_rate_limit() {
        let src = r#"app.post("/api/auth/login", handler);"#;
        assert_eq!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").len(), 1);
    }

    // Regression for #236: an MSW `http.post` mock of an auth endpoint is a
    // test double, not a real route — nothing to rate-limit.
    #[test]
    fn allows_msw_http_post_mock() {
        let src = r#"
            mswServer.use(
                http.post("*/api/v1/auth/sign-in/email", () => HttpResponse.json({ token: "x" })),
            );
        "#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty(), "{:?}", crate::rules::test_helpers::run_rule(&Check, src, "t.tsx"));
    }

    #[test]
    fn allows_wildcard_path_on_other_callee() {
        let src = r#"server.post("*/auth/login", handler);"#;
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty(), "{:?}", crate::rules::test_helpers::run_rule(&Check, src, "t.tsx"));
    }

    // Regression for #3229: an auth route registered in a `*.test.ts` file is
    // exercising the router (e.g. Hono's `basePath()` composition), not deploying
    // a real credential endpoint. Rate-limiting is an operational concern that
    // cannot be enforced on test routes. The engine's central `skip_in_test_dir`
    // gate suppresses the rule for files in test directories.
    #[test]
    fn gated_skips_auth_route_in_test_file() {
        let src = r#"app.get("/login", (c) => c.text("get /login"));"#;
        let d = crate::rules::test_helpers::run_rule_gated(&Check, src, "src/hono.test.ts");
        assert!(d.is_empty(), "{d:?}");
    }

    // The same auth route under a non-test path is shipped code and must still
    // flag — no over-suppression.
    #[test]
    fn gated_still_flags_auth_route_in_source() {
        let src = r#"app.get("/login", (c) => c.text("get /login"));"#;
        let d = crate::rules::test_helpers::run_rule_gated(&Check, src, "src/routes.ts");
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("/login"));
    }

    // Run the rule against `target_rel` with the whole `files` set indexed into
    // a real `ProjectCtx`, so the package-wide global-rate-limit scan sees the
    // sibling setup file. Non-TS/JS entries (e.g. `package.json`) are written to
    // disk to establish package boundaries but not indexed. When no
    // `package.json` is provided, a root manifest is written.
    fn run_in_project(files: &[(&str, &str)], target_rel: &str) -> Vec<Diagnostic> {
        use crate::files::{Language, SourceFile};
        use crate::project::ProjectCtx;
        use std::fs;
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        if !files.iter().any(|(rel, _)| *rel == "package.json") {
            fs::write(dir.path().join("package.json"), r#"{"name":"app"}"#).unwrap();
        }
        let mut source_files: Vec<SourceFile> = Vec::new();
        for (rel, content) in files {
            let p = dir.path().join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, content).unwrap();
            if let Some(language) = Language::from_path(&p) {
                source_files.push(SourceFile { path: p, language });
            }
        }
        let refs: Vec<&SourceFile> = source_files.iter().collect();
        let project = ProjectCtx::load(&refs, &crate::config::Config::default());
        let target = dir.path().join(target_rel);
        let source = fs::read_to_string(&target).unwrap();
        crate::oxc_helpers::reset_file_caches();
        let allocator = oxc_allocator::Allocator::default();
        let source_type = crate::oxc_helpers::source_type_for_path(&target);
        let parse_ret =
            oxc_parser::Parser::new(&allocator, &source, source_type).parse();
        let semantic = oxc_semantic::SemanticBuilder::new()
            .build(&parse_ret.program)
            .semantic;
        let ctx = CheckCtx::for_test_with_project(&target, &source, &project);
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if Check.interested_kinds().contains(&node.kind().ty()) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    // Regression for #5370 (directus/directus): the auth route lives in
    // `controllers/auth.ts`, but the rate limiter is registered globally in a
    // separate `app.ts` via `app.use(rateLimiter)` before the auth router is
    // mounted — so every auth request is rate-limited. The project-wide scan
    // recognizes the global registration and the route is not flagged.
    #[test]
    fn allows_auth_route_with_app_level_global_rate_limit_in_setup_file() {
        let app_ts = r#"
            import { rateLimiter } from "./rate-limiter";
            const app = express();
            app.use(rateLimiter);
            app.use("/auth", authRouter);
        "#;
        let auth_ts = r#"
            router.post("/password/reset", asyncHandler(async (req, _res, next) => {}), respond);
        "#;
        let d = run_in_project(
            &[("src/app.ts", app_ts), ("src/controllers/auth.ts", auth_ts)],
            "src/controllers/auth.ts",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // No rate limiting anywhere — neither per-route on the auth route nor a
    // global `app.use(<rateLimiter>)` in any project file — must still flag.
    #[test]
    fn still_flags_auth_route_with_no_rate_limit_anywhere() {
        let app_ts = r#"
            const app = express();
            app.use("/auth", authRouter);
        "#;
        let auth_ts = r#"
            router.post("/password/reset", asyncHandler(async (req, _res, next) => {}), respond);
        "#;
        let d = run_in_project(
            &[("src/app.ts", app_ts), ("src/controllers/auth.ts", auth_ts)],
            "src/controllers/auth.ts",
        );
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("/password/reset"));
    }

    // A non-rate-limit global middleware (`app.use(cors())`) must NOT count as
    // coverage — only a genuine rate limiter suppresses the diagnostic.
    #[test]
    fn cors_global_middleware_does_not_count_as_rate_limit() {
        let app_ts = r#"
            import cors from "cors";
            const app = express();
            app.use(cors());
            app.use("/auth", authRouter);
        "#;
        let auth_ts = r#"
            router.post("/login", asyncHandler(async (req, _res, next) => {}), respond);
        "#;
        let d = run_in_project(
            &[("src/app.ts", app_ts), ("src/controllers/auth.ts", auth_ts)],
            "src/controllers/auth.ts",
        );
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("/login"));
    }

    // A global limiter in one monorepo package must NOT suppress an unprotected
    // auth route in a different package — the scan is scoped to the auth route's
    // own package boundary.
    #[test]
    fn global_limiter_in_other_package_does_not_suppress() {
        let pkg_a_app = r#"
            const app = express();
            app.use(rateLimiter);
            app.use("/auth", authRouter);
        "#;
        let pkg_b_auth = r#"
            router.post("/login", asyncHandler(async (req, _res, next) => {}), respond);
        "#;
        let d = run_in_project(
            &[
                ("packages/a/package.json", r#"{"name":"a"}"#),
                ("packages/a/src/app.ts", pkg_a_app),
                ("packages/b/package.json", r#"{"name":"b"}"#),
                ("packages/b/src/auth.ts", pkg_b_auth),
            ],
            "packages/b/src/auth.ts",
        );
        assert_eq!(d.len(), 1, "{d:?}");
        assert!(d[0].message.contains("/login"));
    }

    // A global limiter in the same package, with a route in a sibling file,
    // suppresses — the package-scoped scan still finds intra-package setup.
    #[test]
    fn global_limiter_in_same_package_suppresses() {
        let app_ts = r#"
            const app = express();
            app.use(rateLimiter);
            app.use("/auth", authRouter);
        "#;
        let auth_ts = r#"
            router.post("/login", asyncHandler(async (req, _res, next) => {}), respond);
        "#;
        let d = run_in_project(
            &[
                ("packages/api/package.json", r#"{"name":"api"}"#),
                ("packages/api/src/app.ts", app_ts),
                ("packages/api/src/controllers/auth.ts", auth_ts),
            ],
            "packages/api/src/controllers/auth.ts",
        );
        assert!(d.is_empty(), "{d:?}");
    }

    // Non-ASCII source within the 125-byte window after `.use(` (an accented
    // char in a comment) must not panic on a mid-codepoint slice.
    #[test]
    fn non_ascii_near_use_does_not_panic() {
        let src = "app.use(cors()); // configuração de middleware aqui ✓";
        assert!(!has_global_rate_limit(src));
    }
}
