//! node-global-require oxc backend — require() must be at module top level.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Image, font, and media extensions that the Metro/Expo bundler resolves
/// statically when passed to `require()` (the documented React Native pattern).
const STATIC_ASSET_EXTENSIONS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg", ".ttf", ".otf", ".woff", ".woff2",
    ".mp4", ".webm", ".mov", ".m4v", ".mp3", ".wav", ".aac", ".m4a",
];

fn is_static_asset_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    STATIC_ASSET_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// Data-file extensions loaded as values rather than executable modules. A
/// conditional `require()` of one of these is a lazy data-dispatch (load only
/// the requested locale/dataset on demand), not a hoistable module import.
const DATA_FILE_EXTENSIONS: &[&str] = &[".json", ".json5", ".jsonc", ".yaml", ".yml", ".toml"];

/// True when `path` is a relative reference (`./` or `../`) to a local data
/// file. Such a require points at project-local data, not an npm/builtin module,
/// so a conditional one is on-demand data loading rather than a deferrable
/// module import.
fn is_relative_data_file(path: &str) -> bool {
    let is_relative = path.starts_with("./") || path.starts_with("../");
    if !is_relative {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    DATA_FILE_EXTENSIONS.iter().any(|ext| lower.ends_with(ext))
}

/// True when `path` is a bare package specifier: not a relative (`./`, `../`)
/// or absolute (`/`) path, and not a data file. A conditional `require()` of
/// such a specifier is a lazy-load of an optional module dependency — the
/// package is loaded only when a caller opts into a feature, so hoisting it to
/// module top level would eagerly load it on every import, changing semantics.
fn is_bare_package_specifier(path: &str) -> bool {
    if path.starts_with("./") || path.starts_with("../") || path.starts_with('/') {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    DATA_FILE_EXTENSIONS.iter().all(|ext| !lower.ends_with(ext))
}

/// The string-literal argument of a `require()` call, or `None` when the call
/// has no argument or a non-literal (dynamic) argument.
fn string_literal_arg<'a>(call: &'a oxc_ast::ast::CallExpression) -> Option<&'a str> {
    match call.arguments.first() {
        Some(oxc_ast::ast::Argument::StringLiteral(lit)) => Some(lit.value.as_str()),
        _ => None,
    }
}

/// True when the require argument is a dynamic (computed) expression rather than
/// a plain string literal — a template literal with substitutions, a variable,
/// or any other expression. Such a path is only known at runtime and cannot be
/// rewritten as a static top-level import, so the rule's remediation does not
/// apply.
fn has_dynamic_argument(call: &oxc_ast::ast::CallExpression) -> bool {
    let Some(arg) = call.arguments.first() else {
        return false;
    };
    match arg {
        oxc_ast::ast::Argument::StringLiteral(_) => false,
        // A template literal with no substitutions (a single static quasi) is
        // statically known and therefore hoistable; one with expressions is not.
        oxc_ast::ast::Argument::TemplateLiteral(tpl) => !tpl.expressions.is_empty(),
        oxc_ast::ast::Argument::SpreadElement(_) => false,
        _ => true,
    }
}

/// Test-runner lifecycle hooks whose callback bodies legitimately call
/// `require()`: after `jest.resetModules()` / `vi.resetModules()` the module
/// registry is cleared, and a fresh CommonJS `require()` is the only way to
/// re-import the reset module (a static `import` is hoisted and cannot observe
/// the reset). The require lives inside the hook callback by necessity.
const LIFECYCLE_HOOK_IDENTS: &[&str] = &["beforeEach", "beforeAll", "afterEach", "afterAll"];

/// Identifier name of a hook call's callee for the bare (`beforeEach(...)`) and
/// member (`test.beforeEach(...)`) forms; `None` for any other callee shape.
fn hook_callee_name<'a>(call: &'a oxc_ast::ast::CallExpression) -> Option<&'a str> {
    match &call.callee {
        oxc_ast::ast::Expression::Identifier(id) => Some(id.name.as_str()),
        oxc_ast::ast::Expression::StaticMemberExpression(m) => Some(m.property.name.as_str()),
        _ => None,
    }
}

/// True when `func_node` is the callback argument of a test lifecycle hook call
/// (`beforeEach`/`beforeAll`/`afterEach`/`afterAll`). The callback's immediate
/// parent in oxc's semantic tree is the `CallExpression` itself (arguments have
/// no wrapper node); requiring the function to appear in `arguments` excludes an
/// IIFE in the callee position.
fn is_lifecycle_hook_callback(
    func_node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let parent = semantic.nodes().parent_node(func_node.id());
    let AstKind::CallExpression(call) = parent.kind() else {
        return false;
    };
    let Some(name) = hook_callee_name(call) else {
        return false;
    };
    if !LIFECYCLE_HOOK_IDENTS.contains(&name) {
        return false;
    }
    let span = func_node.kind().span();
    call.arguments.iter().any(|arg| arg.span() == span)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["require"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::Identifier(callee) = &call.callee else { return };
        if callee.name.as_str() != "require" {
            return;
        }

        // React Native / Metro bundle static assets via `require("./img.png")`
        // inside JSX — these are bundler-managed asset references, not CommonJS
        // module loads, and the documented pattern requires them inline. Exempt
        // string-literal arguments pointing at a known static-asset extension.
        if let Some(path) = string_literal_arg(call)
            && is_static_asset_path(path)
        {
            return;
        }

        // A dynamic `require(`./locales/${locale}.json`)` / `require(name)` is
        // resolved at runtime and cannot be rewritten as a static top-level
        // import, so the rule's "move to top level" remediation does not apply.
        if has_dynamic_argument(call) {
            return;
        }

        let data_file_arg = string_literal_arg(call).map(is_relative_data_file).unwrap_or(false);
        let bare_pkg_arg = string_literal_arg(call).map(is_bare_package_specifier).unwrap_or(false);

        // Walk ancestors: require is OK if all ancestors are top-level.
        let mut in_function = false;
        let mut first = true;
        for ancestor in semantic.nodes().ancestors(node.id()) {
            if first {
                first = false;
                continue;
            }
            match ancestor.kind() {
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_) => {
                    // A `require()` inside a test lifecycle-hook callback
                    // (`beforeEach`/`beforeAll`/`afterEach`/`afterAll`) is the
                    // documented Jest/Vitest way to re-import a module after
                    // `resetModules()`; do not flag it.
                    if is_lifecycle_hook_callback(ancestor, semantic) {
                        return;
                    }
                    in_function = true;
                    break;
                }
                // A conditional `require()` of either a relative data file or a
                // bare package specifier is a lazy load on demand: a data file
                // loads only the requested locale/dataset, a bare package loads
                // an optional dependency only when a caller opts into a feature.
                // Hoisting either would eagerly load it on every import.
                AstKind::IfStatement(_) | AstKind::SwitchStatement(_)
                    if data_file_arg || bare_pkg_arg =>
                {
                    return;
                }
                AstKind::MethodDefinition(_)
                | AstKind::IfStatement(_)
                | AstKind::ForStatement(_)
                | AstKind::ForInStatement(_)
                | AstKind::WhileStatement(_)
                | AstKind::TryStatement(_)
                | AstKind::SwitchStatement(_) => {
                    in_function = true;
                    break;
                }
                _ => {}
            }
        }

        if !in_function {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Unexpected `require()`. Move it to the top-level module scope.".into(),
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn allows_image_asset_require_in_jsx() {
        let d = run(
            r#"const x = <Image source={require("@/assets/images/partial-react-logo.png")} />;"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_font_asset_require() {
        assert!(run(r#"function f() { return require("./assets/fonts/Inter.ttf"); }"#).is_empty());
    }

    #[test]
    fn flags_module_require_in_function() {
        let d = run(r#"function init() { const fs = require("fs"); return fs; }"#);
        assert_eq!(d.len(), 1);
    }

    // Regression for #1727: `require()` after `jest.resetModules()` inside a
    // `beforeEach` hook is the only way to re-import a reset module.
    #[test]
    fn allows_require_in_before_each_hook() {
        let d = run(
            r#"beforeEach(() => { jest.resetModules(); const m = require("../act-compat").default; });"#,
        );
        assert!(d.is_empty());
    }

    // Regression for #1727: same pattern with `beforeAll`.
    #[test]
    fn allows_require_in_before_all_hook() {
        let d = run(
            r#"beforeAll(() => { process.env.X = "true"; const rtl = require("../"); });"#,
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_require_in_after_each_hook() {
        let d = run(r#"afterEach(() => { const m = require("./reset"); });"#);
        assert!(d.is_empty());
    }

    // Vitest member form `test.beforeEach(() => ...)`.
    #[test]
    fn allows_require_in_member_form_hook() {
        let d = run(r#"test.beforeEach(() => { const m = require("./reset"); });"#);
        assert!(d.is_empty());
    }

    // Negative space: a genuine production `require()` inside a non-hook callback
    // is still flagged — the exemption is scoped to the four lifecycle hooks.
    #[test]
    fn flags_require_in_non_hook_callback() {
        let d = run(r#"setup(() => { const fs = require("fs"); return fs; });"#);
        assert_eq!(d.len(), 1);
    }

    // Regression for #5055: conditional lazy data-dispatch — relative `.json`
    // data files loaded on demand per locale inside an if/else chain. Hoisting
    // would eagerly bundle every locale's data file.
    #[test]
    fn allows_conditional_locale_data_dispatch() {
        let d = run(
            r#"function getHtmlData(lang: string) {
                let data;
                if (lang === 'ja') { data = require('../data/template/ja.json'); }
                else if (lang === 'fr') { data = require('../data/template/fr.json'); }
                else if (lang === 'ko') { data = require('../data/template/ko.json'); }
                return data;
            }"#,
        );
        assert!(d.is_empty());
    }

    // #5055: same lazy data-dispatch expressed as a switch.
    #[test]
    fn allows_switch_locale_data_dispatch() {
        let d = run(
            r#"function load(lang: string) {
                switch (lang) {
                    case 'ja': return require('./locales/ja.json');
                    case 'fr': return require('./locales/fr.json');
                    default: return require('./locales/en.json');
                }
            }"#,
        );
        assert!(d.is_empty());
    }

    // #5055: a dynamic (computed) require path cannot be hoisted to a static
    // top-level import, so it is never flagged.
    #[test]
    fn allows_dynamic_template_literal_require() {
        let d = run(
            r#"function load(locale: string) { return require(`./locales/${locale}.json`); }"#,
        );
        assert!(d.is_empty());
    }

    // #5055: a variable-argument require is also dynamic and not hoistable.
    #[test]
    fn allows_variable_argument_require() {
        let d = run(r#"function load(name: string) { return require(name); }"#);
        assert!(d.is_empty());
    }

    // #6637: a bare specifier inside a conditional is a lazy-load of an optional
    // module dependency. The structural rule does not inspect the name, so a
    // builtin like `fs` is exempt too; the negative space — a relative module in
    // a conditional — stays flagged (`flags_relative_js_module_in_conditional`).
    #[test]
    fn allows_bare_specifier_require_in_conditional() {
        let d = run(r#"function f(x: boolean) { if (x) { const fs = require("fs"); return fs; } }"#);
        assert!(d.is_empty());
    }

    // Regression for #6637 (unjs/jiti src/jiti.ts:56): a synchronous factory
    // lazy-loads an optional peer dependency via a bare package specifier only
    // when the caller opts into the feature. Hoisting it would eagerly load the
    // optional package on every `import`.
    #[test]
    fn allows_conditional_optional_peer_dependency_lazy_load() {
        let d = run(
            r#"function createJiti(opts: { tsconfigPaths?: boolean }) {
                if (opts.tsconfigPaths) {
                    const { getTsconfig, createPathsMatcher } =
                        require("get-tsconfig") as typeof import("get-tsconfig");
                }
            }"#,
        );
        assert!(d.is_empty());
    }

    // #6637: same lazy-load expressed as a switch over a bare package specifier.
    #[test]
    fn allows_switch_bare_package_lazy_load() {
        let d = run(
            r#"function load(kind: string) {
                switch (kind) {
                    case 'a': return require("pkg-a");
                    default: return require("pkg-b");
                }
            }"#,
        );
        assert!(d.is_empty());
    }

    // Negative space for #6637: a bare package `require()` inside a function but
    // NOT inside an if/switch is still flagged — the exemption is scoped to the
    // conditional (lazy-load) position.
    #[test]
    fn flags_unconditional_bare_package_require_in_function() {
        let d = run(r#"function init() { const _ = require("lodash"); return _; }"#);
        assert_eq!(d.len(), 1);
    }

    // Negative space for #5055: a relative `.js` module (executable code, not
    // data) inside a conditional is still flagged — it is hoistable.
    #[test]
    fn flags_relative_js_module_in_conditional() {
        let d = run(
            r#"function f(x: boolean) { if (x) { const m = require("./helper.js"); return m; } }"#,
        );
        assert_eq!(d.len(), 1);
    }

    // Negative space for #5055: a static template literal (no substitutions) is
    // statically known and hoistable, so a code-module one stays flagged.
    #[test]
    fn flags_static_template_literal_module_require() {
        let d = run(r#"function f() { const fs = require(`fs`); return fs; }"#);
        assert_eq!(d.len(), 1);
    }
}
