use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclaration]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["const "])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclaration(decl) = node.kind() else {
            return;
        };

        // Story files (a `*.stories.*` name, or any file inside a `stories/` or
        // `storybook/` directory) hold story-argument fixtures, option lists, and
        // framework-magic names like `__namedExportsOrder` — local story data
        // following camelCase by convention, not application-wide compile-time
        // invariants (issue #1668).
        if ctx.file.path_segments.in_storybook {
            return;
        }

        if !decl.kind.is_const() {
            return;
        }

        let parent = semantic.nodes().parent_node(node.id());
        if !matches!(parent.kind(), AstKind::Program(_) | AstKind::ExportNamedDeclaration(_)) {
            return;
        }

        // SvelteKit route modules (`+page.ts`, `+layout.server.ts`, `+server.ts`,
        // …) expose page options through `const` exports whose names are the
        // framework's protocol: `export const prerender = true`, `ssr`, `csr`.
        // SvelteKit reads them by exact lowercase name, so they cannot be
        // SCREAMING_SNAKE_CASE (issue #1586). The route-file gate keeps the
        // exemption from covering an ordinary lowercase const in the same file.
        let in_svelte_route = ctx
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(crate::rules::path_utils::is_sveltekit_route_file);

        // Mock and fixture files (under `mock/`, `mocks/`, `__mocks__/`,
        // `fixtures/`, `__fixtures__/`) hold value-level mocks of runtime config
        // objects and scenario fixture data. A boolean mock flag mirrors the
        // camelCase property name of the config interface it simulates
        // (`hasPluginDependencies`); renaming it to SCREAMING_SNAKE_CASE breaks
        // that structural correspondence. These are not application-wide
        // compile-time invariants, so the convention does not apply (issue #1591).
        if crate::rules::path_utils::is_mock_or_fixture_dir_path(ctx.path) {
            return;
        }

        for declarator in &decl.declarations {
            let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &declarator.id else {
                continue;
            };

            let name = id.name.as_str();

            if in_svelte_route && is_sveltekit_page_option(name) {
                continue;
            }

            if !is_primitive_init(declarator) {
                continue;
            }

            if is_dom_dimension_name(name) {
                continue;
            }

            // A name carrying a non-ASCII character cannot be expressed in
            // `SCREAMING_SNAKE_CASE`: the convention is an ASCII one, and the
            // uppercase of a Greek letter (`α` → `Α`) is not a form anyone
            // writes. Such names mirror a published spec's symbol notation —
            // `α`/`β` are the ITU-R BT.2020 transfer-function coefficients — so
            // the convention does not apply (issue #5918). This is a pure
            // Unicode property, not a per-symbol allowlist.
            if name.chars().any(|c| !c.is_ascii()) {
                continue;
            }

            if super::is_screaming_snake(name) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, id.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("Top-level constant `{name}` is not in `SCREAMING_SNAKE_CASE`."),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// SvelteKit's documented page-option export names. In a SvelteKit route module
/// these `const` exports form the framework's protocol — SvelteKit reads them by
/// exact lowercase name, so they cannot be renamed to SCREAMING_SNAKE_CASE.
/// See <https://svelte.dev/docs/kit/page-options>.
fn is_sveltekit_page_option(name: &str) -> bool {
    matches!(
        name,
        "prerender" | "ssr" | "csr" | "trailingSlash" | "config" | "actions" | "load" | "entries"
    )
}

/// Canonical DOM, canvas, and viewport dimension property names. In web
/// graphics and creative-coding code (p5.js, d3, Three.js, raw canvas) these
/// `const` bindings mirror the lowercase property names of the platform APIs
/// they parallel — `HTMLCanvasElement.width`/`.height`, `window.innerWidth`,
/// `element.clientWidth`/`.offsetHeight`. Renaming them to SCREAMING_SNAKE_CASE
/// would break the visual correspondence with the API reads they shadow, so
/// this curated set is an accepted lowercase convention (issue #5416). Kept to
/// names that are unambiguously dimension properties — single-letter
/// coordinates and direction words are excluded because they are common magic
/// constants outside graphics code — so ordinary literal constants (`timeout`,
/// `maxRetries`) still require SCREAMING_SNAKE_CASE.
fn is_dom_dimension_name(name: &str) -> bool {
    matches!(
        name,
        "width"
            | "height"
            | "depth"
            | "innerWidth"
            | "innerHeight"
            | "outerWidth"
            | "outerHeight"
            | "clientWidth"
            | "clientHeight"
            | "offsetWidth"
            | "offsetHeight"
    )
}

fn is_primitive_init(declarator: &oxc_ast::ast::VariableDeclarator) -> bool {
    let Some(init) = &declarator.init else {
        return false;
    };
    matches!(
        init,
        oxc_ast::ast::Expression::NumericLiteral(_)
            | oxc_ast::ast::Expression::BooleanLiteral(_)
    ) || is_unary_numeric(init)
        || is_array_of_literals(init)
}

fn is_unary_numeric(expr: &oxc_ast::ast::Expression) -> bool {
    if let oxc_ast::ast::Expression::UnaryExpression(u) = expr {
        return matches!(u.argument, oxc_ast::ast::Expression::NumericLiteral(_));
    }
    false
}

/// Treats an array as a magic-constant literal only when every element is a
/// numeric or boolean literal. Arrays containing string literals are named
/// configuration lists (Vite `optimizeDeps`, allowed-origin lists, feature-flag
/// keys) that follow camelCase by ecosystem convention, so they are exempt.
fn is_array_of_literals(expr: &oxc_ast::ast::Expression) -> bool {
    let oxc_ast::ast::Expression::ArrayExpression(arr) = expr else {
        return false;
    };
    if arr.elements.is_empty() {
        return false;
    }
    arr.elements.iter().all(|el| {
        matches!(
            el,
            oxc_ast::ast::ArrayExpressionElement::NumericLiteral(_)
                | oxc_ast::ast::ArrayExpressionElement::BooleanLiteral(_)
        )
    })
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "sketch.js")
    }

    #[test]
    fn allows_canvas_dimension_constants() {
        // Regression for #5416: `width`/`height` mirror the DOM/canvas API
        // property names and are an accepted lowercase convention.
        assert!(run_on("const width = 640;\nconst height = 480;").is_empty());
    }

    #[test]
    fn allows_viewport_dimension_constants() {
        assert!(run_on("const innerWidth = 1024;\nconst clientHeight = 768;").is_empty());
    }

    #[test]
    fn flags_ordinary_literal_constant() {
        let diags = run_on("const timeout = 3000;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("timeout"));
    }

    #[test]
    fn flags_camel_case_dimension_alias() {
        // `canvasSize`/`rectWidth` are camelCase author choices, not DOM API
        // property names, so they still require SCREAMING_SNAKE_CASE.
        assert_eq!(run_on("const canvasSize = 400;").len(), 1);
        assert_eq!(run_on("const rectWidth = 100;").len(), 1);
    }

    #[test]
    fn flags_single_letter_coordinate_constant() {
        // `x`/`y` are common magic constants outside graphics, so they are not
        // exempt and still require SCREAMING_SNAKE_CASE.
        assert_eq!(run_on("const x = 0;").len(), 1);
    }

    #[test]
    fn exemption_is_confined_to_literal_inits() {
        // The exemption only applies to literal-initialized constants. A
        // non-primitive init never reaches the dimension-name check, so it is
        // not flagged regardless of name (`is_primitive_init` gates first).
        assert!(run_on("const width = computeLayout();").is_empty());
    }

    #[test]
    fn allows_screaming_snake() {
        assert!(run_on("const MAX_RETRIES = 5;").is_empty());
    }

    #[test]
    fn allows_non_ascii_constant_names() {
        // Regression for #5918: Greek-letter spec constants cannot be expressed
        // in SCREAMING_SNAKE_CASE (the convention is ASCII-only), so they are
        // exempt. `α`/`β` are the ITU-R BT.2020 transfer-function coefficients.
        assert!(run_on("const \u{3b1} = 1.09929682680944;").is_empty());
        assert!(run_on("const \u{3b2} = 0.018053968510807;").is_empty());
    }

    #[test]
    fn flags_ascii_spec_names() {
        // The ASCII spec names from #5918 (`kE`, `kCH`, `p`, `d0`) all have a
        // valid SCREAMING_SNAKE_CASE form (`K_E`, `K_CH`, `P`, `D0`), so the
        // convention still applies and they are correctly flagged.
        assert_eq!(run_on("const kE = 1;").len(), 1);
        assert_eq!(run_on("const kCH = 1;").len(), 1);
        assert_eq!(run_on("const p = 134.034;").len(), 1);
        assert_eq!(run_on("const d0 = 1.6295e-11;").len(), 1);
    }
}
