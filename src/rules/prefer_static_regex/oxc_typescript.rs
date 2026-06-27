//! prefer-static-regex — OXC backend.
//! Flag regex literals inside repeatedly-callable functions (recompiled on each
//! call). A regex inside an immediately-invoked function expression (IIFE) is
//! exempt when no non-IIFE function encloses it: a module-scoped IIFE runs once
//! at load time, so its regex literals are already constructed exactly once.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, RegExpFlags};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(regex) = node.kind() else { return };

        if ctx.file.path_segments.in_test_dir {
            return;
        }

        // Build/tooling config files (`webpack.config.js`, `vite.config.ts`, …)
        // are evaluated once at build/startup. A config factory's regex literals
        // are instantiated a single time, not on a hot path, so hoisting them
        // gains nothing while separating each regex from the loader rule it
        // documents.
        if crate::rules::path_utils::is_config_file(ctx.path) {
            return;
        }

        // Walk ancestors to check if inside a repeatedly-callable function. A
        // regex inside an immediately-invoked function expression (IIFE) is
        // constructed once per execution of the IIFE's own enclosing context,
        // not once per call of a reusable function. A module-scoped IIFE runs
        // exactly once at load time, so its regex literals are already
        // effectively static — there is nothing to hoist. Walk outward past IIFE
        // boundaries and treat only a non-IIFE function (which can be called
        // repeatedly) as the per-call recreation site.
        let nodes = semantic.nodes();
        let mut inside_function = false;
        for ancestor in nodes.ancestors(node.id()) {
            match ancestor.kind() {
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                    if crate::oxc_helpers::function_is_immediately_invoked(nodes, ancestor.id()) {
                        continue;
                    }
                    inside_function = true;
                    break;
                }
                _ => {}
            }
        }

        if !inside_function {
            return;
        }

        // A `g`/`y`-flagged regex used as the receiver of `.exec()`/`.test()`
        // is stateful: those methods read and advance the regex's `lastIndex`.
        // Hoisting such a regex to module scope makes that mutable cursor persist
        // across separate calls (and re-entrant use), corrupting iteration — the
        // canonical `while ((m = re.exec(s)))` loop relies on `lastIndex`
        // restarting at 0 on every fresh local instance. Keep it local. Stateless
        // regexes (no `g`/`y`, or used only with `.match`/`.replace`/…) are still
        // suggested for hoisting.
        if regex.regex.flags.intersects(RegExpFlags::G | RegExpFlags::Y)
            && stateful_exec_or_test_usage(node, ctx.source, semantic)
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, regex.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Regex literal inside function is recompiled on each call. Hoist to module scope.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when the regex literal at `node` is the receiver of a `.exec()`/`.test()`
/// call — either inline (`/…/g.exec(s)`) or via a binding (`const re = /…/g;
/// re.exec(s)`). These are the methods that read/advance `lastIndex`, so a
/// `g`/`y`-flagged regex used this way must stay local rather than be hoisted.
fn stateful_exec_or_test_usage<'a>(
    node: &oxc_semantic::AstNode<'a>,
    source: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    if regex_is_inline_exec_test_receiver(node, semantic) {
        return true;
    }
    let Some(var_name) = find_enclosing_binding(node, semantic) else {
        return false;
    };
    let test_pattern = format!("{var_name}.test(");
    let exec_pattern = format!("{var_name}.exec(");
    crate::oxc_helpers::source_contains(source, &test_pattern)
        || crate::oxc_helpers::source_contains(source, &exec_pattern)
}

/// True when the regex literal is the immediate object of a `.exec(...)`/
/// `.test(...)` member call, e.g. `/…/g.exec(s)`.
fn regex_is_inline_exec_test_receiver<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let regex_span = node.kind().span();
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::CallExpression(call) = ancestor.kind() {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                let method = member.property.name.as_str();
                if (method == "exec" || method == "test")
                    && member.object.span() == regex_span
                {
                    return true;
                }
            }
            return false;
        }
    }
    false
}

/// Walk ancestors to find the enclosing `VariableDeclarator` and return the
/// binding identifier name (`const re = /…/g` → `re`).
fn find_enclosing_binding<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a str> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        if let AstKind::VariableDeclarator(decl) = ancestor.kind() {
            if let BindingPattern::BindingIdentifier(id) = &decl.id {
                return Some(id.name.as_str());
            }
            return None;
        }
    }
    None
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

    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, code, "t.ts")
    }

    /// Build a real `FileCtx` from `path` so `ctx.file.path_segments.in_test_dir`
    /// reflects the path's test-directory classification.
    fn run_at(code: &str, path: &str) -> Vec<Diagnostic> {
        use crate::files::Language;
        let path = std::path::Path::new(path);
        let project = crate::project::default_static_project_ctx();
        let lang = Language::from_path(path).unwrap_or(Language::TypeScript);
        let file = crate::rules::file_ctx::FileCtx::build(path, code, lang, project);
        crate::rules::test_helpers::run_rule_with_ctx(&Check, code, path, project, &file)
    }

    #[test]
    fn flags_regex_in_function() {
        assert_eq!(run("function f() { return /abc/.test(s); }").len(), 1);
        assert_eq!(run("const f = () => /abc/.test(s)").len(), 1);
    }

    #[test]
    fn allows_regex_in_module_level_iife_issue_6142() {
        // Issue #6142: regex literals inside an IIFE at module scope run exactly
        // once at module load (when the binding is initialized), so there is
        // nothing to hoist — flagging them is a false positive.
        let code = "const isMacOSWebView = (() =>\n\
                    /Macintosh/.test(ua) &&\n\
                    /AppleWebKit/.test(ua) &&\n\
                    !/Safari/.test(ua))();";
        assert!(run(code).is_empty(), "{:?}", run(code));
        // The classic `function`-expression IIFE form is exempt too.
        let fn_form = "const ok = (function () { return /Macintosh/.test(ua); })();";
        assert!(run(fn_form).is_empty(), "{:?}", run(fn_form));
    }

    #[test]
    fn flags_regex_in_iife_nested_in_function() {
        // Negative space: an IIFE nested inside an ordinary function re-runs on
        // every call of that function, so the regex is still recreated per call
        // and must stay flagged.
        let code = "function f(ua) { return (() => /abc/.test(ua))(); }";
        assert_eq!(run(code).len(), 1, "{:?}", run(code));
    }

    #[test]
    fn flags_regex_in_returned_arrow_from_module_iife() {
        // Negative space: a module-scoped IIFE that *returns* an arrow yields a
        // reusable function; the regex inside that returned arrow is recreated
        // on each call of it, so it stays flagged.
        let code = "const test = (() => (ua) => /abc/.test(ua))();";
        assert_eq!(run(code).len(), 1, "{:?}", run(code));
    }

    #[test]
    fn flags_regex_in_method() {
        let code = "class C { m() { return /abc/.test(s); } }";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn allows_module_level_regex() {
        assert!(run("const RE = /abc/;").is_empty());
        assert!(run("const RE = /abc/g;").is_empty());
    }

    #[test]
    fn allows_class_property_regex() {
        assert!(run("class C { re = /abc/; }").is_empty());
    }

    #[test]
    fn allows_regex_in_test_file() {
        let code = "function f() { return /abc/.test(s); }";
        assert!(run_at(code, "src/foo.test.ts").is_empty());
        assert!(run_at(code, "src/foo.spec.ts").is_empty());
        assert!(run_at(code, "src/__tests__/foo.ts").is_empty());
        assert!(run_at(code, "e2e/foo.ts").is_empty());
        assert!(run_at(code, "tests/foo.ts").is_empty());
    }

    #[test]
    fn allows_regex_in_singular_test_dir() {
        // Regression for issue #1969: pnpm uses a singular `test/` directory
        // convention next to `src/`; a regex inside a function there is a
        // false positive.
        let code = "function f() { return /github\\.com/.test(s); }";
        assert!(run_at(code, "installing/deps-installer/test/install/fromRepo.ts").is_empty());
        assert!(run_at(code, "test/foo.ts").is_empty());
    }

    #[test]
    fn flags_regex_in_non_test_source_file() {
        let code = "function f() { return /abc/.test(s); }";
        assert_eq!(run_at(code, "src/foo.ts").len(), 1);
    }

    #[test]
    fn allows_regex_in_bundler_config_factory() {
        // Regression for issue #5057: a webpack config factory runs once at
        // build startup; its `test` regex literals are not on a hot path.
        let code = "module.exports = (env = {}) => ({ module: { rules: [{ test: /\\.vue$/, use: 'vue-loader' }] } })";
        assert!(run_at(code, "webpack.config.js").is_empty());
        assert!(run_at(code, "vite.config.ts").is_empty());
        assert!(run_at(code, "rollup.config.mjs").is_empty());
    }

    #[test]
    fn allows_global_regex_driving_exec_loop() {
        // Regression for issue #5445: a `/g` regex used with `.exec()` in a
        // `while` loop is stateful (it advances `lastIndex` each call).
        // Hoisting it would persist that cursor across separate calls and
        // corrupt iteration, so it must stay local.
        let code = "function findAllBrackets(v) {\n\
                    \tconst ANGLED = /<([^>]+)>/g;\n\
                    \tlet m;\n\
                    \twhile ((m = ANGLED.exec(v))) { res.push(m); }\n\
                    }";
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_global_regex_with_test() {
        // A `/g` regex used as the receiver of `.test()` is stateful too.
        let code = "function f(s) { const re = /a/g; return re.test(s); }";
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_inline_global_regex_exec() {
        // Inline `/…/g.exec(s)` — the literal is the immediate receiver.
        let code = "function f(s) { let m; while ((m = /a/g.exec(s))) {} }";
        assert!(run(code).is_empty());
    }

    #[test]
    fn allows_sticky_regex_with_exec() {
        // The sticky `/y` flag is stateful the same way as `/g`.
        let code = "function f(s) { const re = /a/y; return re.exec(s); }";
        assert!(run(code).is_empty());
    }

    #[test]
    fn flags_global_regex_used_with_replace() {
        // A `/g` regex used only with `.replace` (not `.exec`/`.test`) has no
        // cross-call `lastIndex` hazard — still suggest hoisting.
        let code = "function f(s) { const re = /a/g; return s.replace(re, 'b'); }";
        assert_eq!(run(code).len(), 1);
    }

    #[test]
    fn flags_global_regex_not_used_statefully() {
        // A `/g` regex never used with `.exec`/`.test` is hoistable.
        let code = "function f(s) { const re = /a/g; return s.match(re); }";
        assert_eq!(run(code).len(), 1);
    }
}
