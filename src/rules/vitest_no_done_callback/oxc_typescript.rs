//! vitest-no-done-callback oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BindingPattern, Expression, FormalParameters};
use std::path::Path;
use std::sync::Arc;

pub struct Check;

const TEST_FUNCTIONS: &[&str] = &[
    "test",
    "it",
    "beforeEach",
    "afterEach",
    "beforeAll",
    "afterAll",
];

/// Karma config filenames that mark a subtree as running under Karma/Jasmine,
/// where the `done` callback is the canonical async API rather than a Vitest
/// anti-pattern.
const KARMA_CONFIG_NAMES: &[&str] =
    &["karma.conf.js", "karma.conf.ts", "karma.conf.cjs", "karma.conf.mjs"];

/// True when the callback's *first* parameter is named `done`.
///
/// Vitest/Jest's legacy callback style passes `done` as the sole parameter
/// (`test("x", (done) => …)`). The `node:test` runner instead passes the test
/// context first and the completion callback second (`test("x", (t, done) => …)`),
/// where `done` is a supported part of its API. Gating on the *first* parameter
/// fires only for the Jest-legacy shape and leaves the `node:test` signature
/// untouched.
fn first_param_is_done(params: &FormalParameters) -> bool {
    match params.items.first().map(|p| &p.pattern) {
        Some(BindingPattern::BindingIdentifier(id)) => id.name.as_str() == "done",
        _ => false,
    }
}

/// True when `dir` holds any `karma.conf.*` file.
fn dir_has_karma_config(dir: &Path) -> bool {
    KARMA_CONFIG_NAMES.iter().any(|name| dir.join(name).is_file())
}

/// True when `path` sits in a subtree configured to run under Karma/Jasmine —
/// i.e. a `karma.conf.*` exists in its directory or any ancestor up to (and
/// including) the project root. In such a subtree the test runner is Jasmine,
/// not Vitest, and `done` is the canonical async callback.
///
/// The walk is bounded by the nearest `package.json` directory; when no
/// manifest is found it falls back to walking to the filesystem root.
fn runs_under_karma(ctx: &CheckCtx, path: &Path) -> bool {
    let Some(start) = path.parent() else {
        return false;
    };
    let root = ctx.project.nearest_package_json_dir(path);
    for dir in start.ancestors() {
        if dir_has_karma_config(dir) {
            return true;
        }
        if root.as_deref() == Some(dir) {
            break;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["done"])
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
        let callee_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::StaticMemberExpression(m) => m.property.name.as_str(),
            _ => return,
        };
        if !TEST_FUNCTIONS.contains(&callee_name) {
            return;
        }
        // Last argument is the callback.
        let Some(arg) = call.arguments.last() else {
            return;
        };
        let params = match arg {
            Argument::ArrowFunctionExpression(a) => &a.params,
            Argument::FunctionExpression(f) => &f.params,
            _ => return,
        };
        if !first_param_is_done(params) {
            return;
        }
        // The `done` callback is a deprecation that is specific to Vitest; under
        // Mocha, Jasmine, or `node:test` it is the canonical async API. Fire only
        // where Vitest is the governing runner, and stay silent when the runner
        // is something else or cannot be determined.
        if !ctx.project.uses_vitest(ctx.path) {
            return;
        }
        if runs_under_karma(ctx, ctx.path) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`done` callback is a Jest legacy pattern — Vitest will never \
                      finish the test. Return a Promise or mark the callback async."
                .into(),
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
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use tempfile::TempDir;

    const VITEST_PKG: &str =
        r#"{"name":"app","devDependencies":{"vitest":"^1.0.0"}}"#;

    /// Run the check against a `.test.ts` file in a temp project that declares
    /// Vitest, so the Vitest gate sees a real manifest on disk. Use for tests
    /// that exercise the AST logic itself (the runner must be Vitest for the
    /// rule to fire at all).
    fn run(src: &str) -> Vec<Diagnostic> {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), VITEST_PKG).unwrap();
        let file = dir.path().join("app.test.ts");
        std::fs::write(&file, src).unwrap();
        run_on_disk(&file, src)
    }

    /// Run the check against an on-disk `path` so the Karma-config walk and the
    /// Vitest gate can see sibling files. Mirrors the production per-node
    /// dispatch.
    fn run_on_disk(path: &Path, src: &str) -> Vec<Diagnostic> {
        crate::oxc_helpers::reset_file_caches();
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, src, SourceType::ts()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test(path, src);
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if Check.interested_kinds().contains(&node.kind().ty()) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    #[test]
    fn flags_done_callback_in_test() {
        let src = r#"test("x", (done) => { setTimeout(done, 100); });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_done_in_beforeEach() {
        let src = r#"beforeEach((done) => { setup(done); });"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_promise_callback() {
        let src = r#"test("x", async () => { await waitFor(); });"#;
        assert!(run(src).is_empty());
    }

    // Regression #1640: `node:test` uses a `(t, done)` signature where the test
    // context comes first and `done` is the second, officially-supported
    // completion callback. This is not the Jest-legacy `(done)` shape and must
    // not be flagged.
    #[test]
    fn allows_node_test_context_done_signature_issue_1640() {
        let src = r#"test('handles null', (t, done) => { t.plan(1); done(); });"#;
        assert!(
            run(src).is_empty(),
            "node:test (t, done) signature must not be flagged"
        );
    }

    // A genuine Jest/Vitest legacy `(done)` callback — `done` as the sole, first
    // parameter — must still fire.
    #[test]
    fn flags_done_as_sole_param() {
        let src = r#"it("does a thing", (done) => { setTimeout(done, 10); });"#;
        assert_eq!(run(src).len(), 1);
    }

    // Regression #1747: a test file under a directory configured to run with
    // Karma/Jasmine (a sibling `karma.conf.js`) legitimately uses the `done`
    // callback — Jasmine's canonical async API — and must not be flagged.
    #[test]
    fn allows_done_in_karma_jasmine_subtree_issue_1747() {
        let dir = TempDir::new().unwrap();
        // Vitest is declared at the root, so the carve-out under test is the
        // Karma subtree exemption — not the absence of a Vitest runner.
        std::fs::write(dir.path().join("package.json"), VITEST_PKG).unwrap();
        let test_dir = dir.path().join("test").join("transition");
        std::fs::create_dir_all(&test_dir).unwrap();
        std::fs::write(
            test_dir.join("karma.conf.js"),
            "module.exports = function (config) { config.set({ frameworks: ['jasmine'] }) }",
        )
        .unwrap();
        let file = test_dir.join("helpers.ts");
        let source = r#"
afterEach(done => {
  const warned = msg =>
    asserted.some(assertedMsg => msg.toString().indexOf(assertedMsg) > -1)
  let count = console.error.calls.count()
  let args
  while (count--) {
    args = console.error.calls.argsFor(count)
    if (!warned(args[0])) {
      done.fail(`Unexpected console.error message: ${args[0]}`)
      return
    }
  }
  done()
})
"#;
        std::fs::write(&file, source).unwrap();
        let diags = run_on_disk(&file, source);
        assert!(
            diags.is_empty(),
            "Karma/Jasmine test file must not be flagged, got {diags:?}"
        );
    }

    // A `done` callback in a Vitest project with no Karma config in any
    // ancestor is a genuine Vitest anti-pattern and must still fire.
    #[test]
    fn flags_done_without_karma_config() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), VITEST_PKG).unwrap();
        let file = dir.path().join("helpers.test.ts");
        let source = r#"afterEach(done => { done() })"#;
        std::fs::write(&file, source).unwrap();
        assert_eq!(run_on_disk(&file, source).len(), 1);
    }

    // Regression #2343: a Mocha project (only `mocha` in devDependencies, no
    // Vitest dependency or config) uses `done` callbacks as its canonical async
    // API. The rule encodes a Vitest-specific deprecation and must stay silent.
    #[test]
    fn allows_done_in_mocha_project_issue_2343() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"app","devDependencies":{"mocha":"^10.0.0","@types/mocha":"^10.0.0"}}"#,
        )
        .unwrap();
        let test_dir = dir.path().join("test");
        std::fs::create_dir_all(&test_dir).unwrap();
        let file = test_dir.join("todo.tests.ts");
        let source = r#"
describe("todo management", () => {
  beforeEach((done) => { done(); });
  it("creates a todo", (done) => { done(); });
});
"#;
        std::fs::write(&file, source).unwrap();
        let diags = run_on_disk(&file, source);
        assert!(
            diags.is_empty(),
            "Mocha `done` callbacks must not be flagged, got {diags:?}"
        );
    }

    // Negative-space guard: a `done` callback in a project that DOES use Vitest
    // (vitest in devDependencies) is the genuine anti-pattern and must fire.
    #[test]
    fn flags_done_in_vitest_project_issue_2343() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"name":"app","devDependencies":{"vitest":"^1.0.0"}}"#,
        )
        .unwrap();
        let test_dir = dir.path().join("test");
        std::fs::create_dir_all(&test_dir).unwrap();
        let file = test_dir.join("todo.test.ts");
        let source = r#"it("creates a todo", (done) => { done(); });"#;
        std::fs::write(&file, source).unwrap();
        assert_eq!(
            run_on_disk(&file, source).len(),
            1,
            "Vitest `done` callback must still be flagged"
        );
    }

    // The exemption applies to the whole subtree: a `karma.conf.js` in a parent
    // directory covers nested spec files.
    #[test]
    fn allows_done_in_nested_karma_subtree() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), VITEST_PKG).unwrap();
        let karma_root = dir.path().join("test");
        let nested = karma_root.join("transition").join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(karma_root.join("karma.conf.js"), "module.exports = {}").unwrap();
        let file = nested.join("scroll.spec.ts");
        let source = r#"it("scrolls", done => { done() })"#;
        std::fs::write(&file, source).unwrap();
        assert!(run_on_disk(&file, source).is_empty());
    }
}
