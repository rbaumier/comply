//! OxcCheck backend for ts-no-export-equal — flag CommonJS-style `export = X`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSExportAssignment]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSExportAssignment(export) = node.kind() else { return };

        // ESLint loads plugins via CommonJS `require()` and reads
        // `module.exports`. A TS plugin authored with `export default {...}`
        // compiles to `exports.default`, which the loader never picks up, so
        // `export = { rules, configs, ... }` (→ `module.exports = ...`) is the
        // required form. Exempt it when the package is an eslint-plugin and the
        // exported value has the plugin object shape.
        if is_eslint_plugin_export(export, ctx) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, export.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "CommonJS-style `export = ...` — use `export default` or named exports."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when `export = <expr>` is the required ESLint-plugin entry export:
/// the package is an eslint-plugin AND the exported value is an object literal
/// carrying at least one ESLint-plugin key (`rules`/`configs`/`processors`/
/// `meta`). Both gates must hold so an unrelated `export = SomeClass` inside an
/// eslint-plugin package stays flagged.
fn is_eslint_plugin_export(
    export: &oxc_ast::ast::TSExportAssignment,
    ctx: &CheckCtx,
) -> bool {
    let Some(pkg) = ctx.project.nearest_package_json(ctx.path) else {
        return false;
    };
    let is_eslint_plugin = pkg
        .name
        .as_deref()
        .is_some_and(is_eslint_plugin_package_name)
        || pkg.peer_dependencies.contains_key("eslint");
    is_eslint_plugin && has_eslint_plugin_shape(&export.expression)
}

/// True for `eslint-plugin-foo`, `@scope/eslint-plugin`, or
/// `@scope/eslint-plugin-foo`.
fn is_eslint_plugin_package_name(name: &str) -> bool {
    let bare = name.strip_prefix('@').and_then(|s| s.split_once('/')).map_or(name, |(_, rest)| rest);
    bare == "eslint-plugin" || bare.starts_with("eslint-plugin-")
}

/// True when the expression is an object literal with at least one key
/// characteristic of an ESLint plugin entry object.
fn has_eslint_plugin_shape(expr: &Expression) -> bool {
    const PLUGIN_KEYS: [&str; 4] = ["rules", "configs", "processors", "meta"];
    let Expression::ObjectExpression(obj) = expr else {
        return false;
    };
    obj.properties.iter().any(|prop| {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            return false;
        };
        let key = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return false,
        };
        PLUGIN_KEYS.contains(&key)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser as OxcParser;
    use oxc_semantic::SemanticBuilder;
    use oxc_span::SourceType;
    use std::fs;
    use tempfile::TempDir;

    fn run_oxc_in_project(path: &std::path::Path, source: &str) -> Vec<Diagnostic> {
        crate::oxc_helpers::reset_file_caches();
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, source, SourceType::ts()).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;
        let ctx = CheckCtx::for_test(path, source);
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let ty = node.kind().ty();
            if Check.interested_kinds().contains(&ty) {
                Check.run(node, &ctx, &semantic, &mut diagnostics);
            }
        }
        diagnostics
    }

    // Regression #2288: an eslint-plugin entry (identified by package name)
    // requires `export = { rules, configs }` for CJS plugin resolution.
    #[test]
    fn allows_eslint_plugin_entry_by_package_name() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"eslint-plugin-ngrx"}"#,
        )
        .unwrap();
        let src_dir = dir.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let file = src_dir.join("index.ts");
        let source = "export = {\n  configs: {},\n  rules: {},\n};";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "eslint-plugin entry `export = {{rules, configs}}` must not be flagged, got {diags:?}"
        );
    }

    #[test]
    fn allows_eslint_plugin_entry_by_eslint_peer_dep() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"@my/lint","peerDependencies":{"eslint":"^9"}}"#,
        )
        .unwrap();
        let file = dir.path().join("index.ts");
        let source = "export = {\n  rules: {},\n};";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert!(
            diags.is_empty(),
            "eslint peer-dep plugin entry must not be flagged, got {diags:?}"
        );
    }

    // Negative space: a plain `export =` in an eslint-plugin package whose value
    // is NOT a plugin object stays flagged (only the plugin entry shape is exempt).
    #[test]
    fn flags_non_plugin_shape_in_eslint_plugin_package() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"eslint-plugin-ngrx"}"#,
        )
        .unwrap();
        let file = dir.path().join("util.ts");
        let source = "class Foo {}\nexport = Foo;";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "non-plugin `export = Foo` must stay flagged, got {diags:?}"
        );
    }

    // Negative space: a plain `export = SomeValue` in an ordinary package with no
    // eslint-plugin signal stays flagged.
    #[test]
    fn flags_export_equal_in_ordinary_package() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"ordinary"}"#).unwrap();
        let file = dir.path().join("index.ts");
        let source = "const x = 1;\nexport = x;";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(diags.len(), 1, "ordinary `export = x` must be flagged, got {diags:?}");
    }

    // Negative space: even a plugin-shaped object stays flagged when the package
    // is not an eslint-plugin (no name prefix, no eslint peer dep).
    #[test]
    fn flags_plugin_shape_in_ordinary_package() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"ordinary"}"#).unwrap();
        let file = dir.path().join("index.ts");
        let source = "export = {\n  rules: {},\n  configs: {},\n};";
        fs::write(&file, source).unwrap();
        let diags = run_oxc_in_project(&file, source);
        assert_eq!(
            diags.len(),
            1,
            "plugin-shaped object outside an eslint-plugin package must be flagged, got {diags:?}"
        );
    }

    #[test]
    fn allows_scoped_eslint_plugin_name() {
        assert!(is_eslint_plugin_package_name("@typescript-eslint/eslint-plugin"));
        assert!(is_eslint_plugin_package_name("@scope/eslint-plugin-foo"));
        assert!(is_eslint_plugin_package_name("eslint-plugin-ngrx"));
        assert!(!is_eslint_plugin_package_name("eslint"));
        assert!(!is_eslint_plugin_package_name("@scope/some-lib"));
    }
}
