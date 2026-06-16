//! Regression tests recovering Biome's `useQwikLoaderLocation` fixtures. The
//! rule is path-sensitive: route loaders/actions must live in a route boundary
//! file (`index`/`layout`/`plugin` under `src/routes`).

use super::oxc_typescript::Check;
use crate::diagnostic::Diagnostic;

fn run(src: &str, path: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&Check, src, path)
}

const IMPORT: &str = "import { routeLoader$ } from '@builder.io/qwik-city';\n";

// ── invalid: loader declared outside a route boundary ───────────────────────

#[test]
fn flags_route_loader_outside_routes_dir() {
    let src = format!("{IMPORT}export const useProductDetails = routeLoader$(async () => {{}});");
    let diags = run(&src, "src/components/product/product.jsx");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("outside of the route boundaries"));
}

// ── invalid: not exported ───────────────────────────────────────────────────

#[test]
fn flags_missing_export() {
    let src = format!("{IMPORT}const useFormLoader = routeLoader$(() => null);");
    let diags = run(&src, "src/routes/index.jsx");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("not being exported"));
}

// ── invalid: wrong name ─────────────────────────────────────────────────────

#[test]
fn flags_non_use_prefixed_name() {
    let src = format!("{IMPORT}export const getProductDetails = routeLoader$(async () => {{}});");
    let diags = run(&src, "src/routes/index.jsx");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("use*"));
}

// ── invalid: argument is a reference, not an inline arrow ────────────────────

#[test]
fn flags_reference_argument() {
    let src = format!(
        "{IMPORT}async function fetcher() {{}}\nexport const useProductDetails = routeLoader$(fetcher);"
    );
    let diags = run(&src, "src/routes/index.jsx");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("inline the arrow function"));
}

// ── valid: exported via a separate `export { x }` ───────────────────────────

#[test]
fn allows_separate_export_specifier() {
    let src =
        format!("{IMPORT}export {{ useFormLoader }};\nconst useFormLoader = routeLoader$(() => null);");
    assert!(run(&src, "src/routes/index.jsx").is_empty());
}

// ── valid: inline export, use-name, inline arrow, route boundary ────────────

#[test]
fn allows_well_formed_loader() {
    let src = format!("{IMPORT}export const useProductDetails = routeLoader$(async () => {{}});");
    assert!(run(&src, "src/routes/index.jsx").is_empty());
}

// ── route boundary variants ─────────────────────────────────────────────────

#[test]
fn allows_loader_in_nested_route_dir() {
    let src = format!("{IMPORT}export const useProducts = routeLoader$(async () => {{}});");
    assert!(run(&src, "src/routes/products/index.jsx").is_empty());
}

#[test]
fn allows_loader_in_layout_file() {
    let src = format!("{IMPORT}export const useProducts = routeLoader$(async () => {{}});");
    assert!(run(&src, "src/routes/layout.tsx").is_empty());
}

#[test]
fn allows_loader_in_plugin_file() {
    let src = format!("{IMPORT}export const useProducts = routeLoader$(async () => {{}});");
    assert!(run(&src, "src/routes/plugin@auth.ts").is_empty());
}

#[test]
fn flags_loader_in_non_boundary_route_file() {
    // Inside `src/routes` but not an index/layout/plugin file.
    let src = format!("{IMPORT}export const useProducts = routeLoader$(async () => {{}});");
    let diags = run(&src, "src/routes/products/data.tsx");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("outside of the route boundaries"));
}

// ── globalAction$ is exempt from the location check ─────────────────────────

#[test]
fn allows_global_action_outside_routes() {
    let src = "import { globalAction$ } from '@builder.io/qwik-city';\n\
               export const useLogin = globalAction$(async () => {});";
    assert!(run(src, "src/components/login.tsx").is_empty());
}

#[test]
fn flags_global_action_missing_use_prefix() {
    let src = "import { globalAction$ } from '@builder.io/qwik-city';\n\
               export const login = globalAction$(async () => {});";
    let diags = run(src, "src/components/login.tsx");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("use*"));
}

// ── route action is governed by the location check ──────────────────────────

#[test]
fn flags_route_action_outside_routes() {
    let src = "import { routeAction$ } from '@builder.io/qwik-city';\n\
               export const useAddToCart = routeAction$(async () => {});";
    let diags = run(src, "src/components/cart.tsx");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("outside of the route boundaries"));
}

// ── Qwik import gate / over-firing guards ───────────────────────────────────

#[test]
fn ignores_same_named_helper_without_qwik_import() {
    // `routeLoader$` here is a local helper, not the Qwik one.
    let src = "function routeLoader$(fn) { return fn; }\n\
               const data = routeLoader$(() => null);";
    assert!(run(src, "src/components/data.tsx").is_empty());
}

#[test]
fn ignores_helper_imported_from_other_package() {
    let src = "import { routeLoader$ } from 'some-other-lib';\n\
               const data = routeLoader$(() => null);";
    assert!(run(src, "src/components/data.tsx").is_empty());
}

#[test]
fn ignores_type_only_import() {
    let src = "import type { routeLoader$ } from '@builder.io/qwik-city';\n\
               const data = routeLoader$(() => null);";
    assert!(run(src, "src/routes/index.ts").is_empty());
}

#[test]
fn honors_aliased_import() {
    // `import { routeLoader$ as loader }` keeps the Qwik classification.
    let src = "import { routeLoader$ as loader } from '@builder.io/qwik-city';\n\
               export const getData = loader(async () => {});";
    let diags = run(src, "src/routes/index.tsx");
    assert_eq!(diags.len(), 1);
    assert!(diags[0].message.contains("use*"));
}
