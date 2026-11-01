//! What the Vitest config says about RTL's cleanup.
//!
//! RTL registers `afterEach(cleanup)` only when `afterEach` is global.
//! `test.globals: true` makes it global.
//! A setup file can register the cleanup instead.

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrayExpressionElement, Expression, ObjectExpression, ObjectPropertyKind, Statement,
};
use oxc_parser::Parser;
use rustc_hash::FxHashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Whether something already runs `cleanup` after each test.
#[derive(Debug, PartialEq, Eq)]
pub enum AutoCleanup {
    /// It does; the text names what does it.
    Registered(String),
    /// Nothing does, so the spec's own `cleanup` is required.
    Absent,
    /// comply cannot tell; the text says why.
    Unknown(String),
}

/// Extensions Vitest accepts for a config or a setup file.
const MODULE_EXTENSIONS: &[&str] = &["ts", "mts", "cts", "js", "mjs", "cjs", "tsx", "jsx"];

/// Workspace files that split the config into projects.
const WORKSPACE_STEMS: &[&str] = &["vitest.workspace"];

/// The largest file comply reads here, in bytes.
const SOURCE_CAP: u64 = 1024 * 1024;

/// The most setup files comply reads for one config.
const SETUP_FILES_CAP: usize = 32;

/// What the config of `test_file` says about RTL's cleanup.
/// The walk stops at `stop_at`, the project root.
/// `package_dir` holds the manifest whose scripts run Vitest.
pub fn auto_cleanup(
    test_file: &Path,
    stop_at: Option<&Path>,
    package_dir: Option<&Path>,
) -> AutoCleanup {
    // A command-line flag overrides the config.
    if package_dir.is_some_and(scripts_pass_globals) {
        return AutoCleanup::Registered("`vitest --globals` in a `package.json` script".into());
    }
    let Some((config, options)) = find_config(test_file, stop_at) else {
        return AutoCleanup::Unknown("no Vitest config applies to this file".into());
    };
    let name = file_name(&config);
    let options = match options {
        Ok(options) => options,
        Err(why) => return AutoCleanup::Unknown(format!("`{name}` {why}")),
    };
    if options.globals {
        return AutoCleanup::Registered(format!("`test.globals: true` in `{name}`"));
    }
    if options.setup_files.len() > SETUP_FILES_CAP {
        return AutoCleanup::Unknown(format!("`{name}` lists over {SETUP_FILES_CAP} setup files"));
    }
    let config_dir = config.parent().unwrap_or(Path::new(""));
    for spec in &options.setup_files {
        let setup = resolve_module(config_dir, spec);
        let Some((setup, source)) =
            setup.and_then(|path| read_capped(&path).map(|text| (path, text)))
        else {
            return AutoCleanup::Unknown(format!("setup file `{spec}` of `{name}` cannot be read"));
        };
        if registers_cleanup(&source) {
            return AutoCleanup::Registered(format!("setup file `{}`", file_name(&setup)));
        }
    }
    AutoCleanup::Absent
}

/// A config's `test` options, or why comply cannot read them.
type Options = Result<TestOptions, &'static str>;

/// The `test` options comply reads.
#[derive(Debug, Default)]
struct TestOptions {
    /// `globals` defaults to false.
    globals: bool,
    setup_files: Vec<String>,
}

/// The nearest config that applies, with its `test` options.
/// `None` when no config sits between the file and `stop_at`.
fn find_config(test_file: &Path, stop_at: Option<&Path>) -> Option<(PathBuf, Options)> {
    for dir in test_file.ancestors().skip(1) {
        if let Some(found) = config_in(dir) {
            return Some(found);
        }
        if stop_at == Some(dir) {
            return None;
        }
    }
    None
}

/// The config of one directory; `vitest.config.*` wins over `vite.config.*`.
fn config_in(dir: &Path) -> Option<(PathBuf, Options)> {
    if let Some(workspace) = existing_module(dir, WORKSPACE_STEMS) {
        return Some((workspace, Err("splits the config into projects")));
    }
    if let Some(config) = existing_module(dir, &["vitest.config"]) {
        let options = read_test_options(&config).map(Option::unwrap_or_default);
        return Some((config, options));
    }
    let config = existing_module(dir, &["vite.config"])?;
    match read_test_options(&config) {
        // A `vite.config.*` without `test` is a build config only.
        Ok(None) => None,
        options => Some((config, options.map(Option::unwrap_or_default))),
    }
}

/// The first `<stem>.<extension>` file in `dir`.
fn existing_module(dir: &Path, stems: &[&str]) -> Option<PathBuf> {
    stems.iter().find_map(|stem| {
        MODULE_EXTENSIONS
            .iter()
            .map(|extension| dir.join(format!("{stem}.{extension}")))
            .find(|path| path.is_file())
    })
}

/// The `test` block of `config`'s default export.
/// `Ok(None)` when the export has no `test` key.
fn read_test_options(config: &Path) -> Result<Option<TestOptions>, &'static str> {
    let source = read_capped(config).ok_or("cannot be read")?;
    let allocator = Allocator::default();
    let source_type = crate::oxc_helpers::source_type_for_path(config);
    let parsed = Parser::new(&allocator, &source, source_type).parse();
    if parsed.panicked || !parsed.errors.is_empty() {
        return Err("does not parse");
    }
    let export = parsed
        .program
        .body
        .iter()
        .find_map(|statement| match statement {
            Statement::ExportDefaultDeclaration(export) => Some(&export.declaration),
            _ => None,
        })
        .ok_or("has no `export default`")?;
    let object = export
        .as_expression()
        .and_then(config_object)
        .ok_or("exports no object literal")?;
    let Some(test) = static_property(object)?.remove("test") else {
        return Ok(None);
    };
    let Expression::ObjectExpression(test) = test.without_parentheses() else {
        return Err("sets `test` to no object literal");
    };
    test_options(test).map(Some)
}

/// The object literal a config exports.
/// It sees through `defineConfig`, `defineProject`, and an arrow returning it.
fn config_object<'e, 'a>(expression: &'e Expression<'a>) -> Option<&'e ObjectExpression<'a>> {
    match expression.without_parentheses() {
        Expression::ObjectExpression(object) => Some(object),
        Expression::CallExpression(call) => {
            let Expression::Identifier(callee) = &call.callee else {
                return None;
            };
            let [argument] = call.arguments.as_slice() else {
                return None;
            };
            matches!(callee.name.as_str(), "defineConfig" | "defineProject")
                .then(|| argument.as_expression().and_then(config_object))
                .flatten()
        }
        Expression::ArrowFunctionExpression(arrow) => {
            arrow.get_expression().and_then(config_object)
        }
        Expression::TSSatisfiesExpression(inner) => config_object(&inner.expression),
        Expression::TSAsExpression(inner) => config_object(&inner.expression),
        _ => None,
    }
}

/// Each statically named property of `object`, by name.
/// A spread or a computed key fails.
/// Either could set anything.
fn static_property<'e, 'a>(
    object: &'e ObjectExpression<'a>,
) -> Result<FxHashMap<String, &'e Expression<'a>>, &'static str> {
    object
        .properties
        .iter()
        .map(|property| match property {
            ObjectPropertyKind::ObjectProperty(property) => property
                .key
                .static_name()
                .map(|name| (name.to_string(), &property.value))
                .ok_or("has a computed key"),
            ObjectPropertyKind::SpreadProperty(_) => Err("spreads an object comply cannot see"),
        })
        .collect()
}

/// `globals` and `setupFiles` of a `test` block.
fn test_options(test: &ObjectExpression) -> Result<TestOptions, &'static str> {
    let properties = static_property(test)?;
    if properties.contains_key("projects") || properties.contains_key("workspace") {
        return Err("splits the config into projects");
    }
    let globals = match properties
        .get("globals")
        .map(|value| value.without_parentheses())
    {
        None => false,
        Some(Expression::BooleanLiteral(literal)) => literal.value,
        Some(_) => return Err("sets `test.globals` to no literal"),
    };
    let setup_files = match properties
        .get("setupFiles")
        .map(|value| value.without_parentheses())
    {
        None => Vec::new(),
        Some(Expression::StringLiteral(spec)) => vec![spec.value.to_string()],
        Some(Expression::ArrayExpression(array)) => array
            .elements
            .iter()
            .map(|element| match element {
                ArrayExpressionElement::StringLiteral(spec) => Ok(spec.value.to_string()),
                _ => Err("lists a `test.setupFiles` entry that is no string"),
            })
            .collect::<Result<_, _>>()?,
        Some(_) => return Err("sets `test.setupFiles` to no literal"),
    };
    Ok(TestOptions {
        globals,
        setup_files,
    })
}

/// The file `spec` names, relative to `dir`, extension optional.
fn resolve_module(dir: &Path, spec: &str) -> Option<PathBuf> {
    let path = dir.join(spec);
    if path.is_file() {
        return Some(path);
    }
    MODULE_EXTENSIONS
        .iter()
        .map(|extension| dir.join(format!("{spec}.{extension}")))
        .find(|candidate| candidate.is_file())
}

/// Whether a setup file runs RTL's `cleanup` after each test.
/// It must name `afterEach`, `cleanup` and a Testing Library module.
fn registers_cleanup(source: &str) -> bool {
    ["afterEach", "cleanup", "@testing-library/"]
        .iter()
        .all(|needle| source.contains(needle))
}

/// Whether a script of `package_dir`'s manifest runs `vitest --globals`.
fn scripts_pass_globals(package_dir: &Path) -> bool {
    let Some(raw) = read_capped(&package_dir.join("package.json")) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    manifest
        .get("scripts")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|scripts| {
            scripts
                .values()
                .filter_map(serde_json::Value::as_str)
                .any(|command| {
                    command.contains("vitest")
                        && command.split_whitespace().any(|word| word == "--globals")
                })
        })
}

/// `path`'s text, or `None` when unreadable or over [`SOURCE_CAP`].
fn read_capped(path: &Path) -> Option<String> {
    let mut text = String::new();
    File::open(path)
        .ok()?
        .take(SOURCE_CAP + 1)
        .read_to_string(&mut text)
        .ok()?;
    u64::try_from(text.len())
        .is_ok_and(|len| len <= SOURCE_CAP)
        .then_some(text)
}

/// `path`'s last component, for a message.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}
