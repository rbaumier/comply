//! React Router 7 in framework mode.
//! It loads some files by convention, never by `import`.
//! `unused-file` takes them as entry points.
//! `dead-export` knows the exports React Router reads by name.

use crate::oxc_helpers::with_oxc_parse;
use globset::{GlobBuilder, GlobSetBuilder};
use oxc_ast::AstKind;
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, Expression, ImportDeclarationSpecifier, ObjectPropertyKind,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::{Path, PathBuf};

/// Extensions React Router accepts for its config and convention files.
const EXTENSIONS: [&str; 6] = ["tsx", "ts", "jsx", "js", "mts", "mjs"];
/// Extensions `flatRoutes()` accepts for the module of a route folder.
const FOLDER_MODULE_EXTENSIONS: [&str; 6] = ["js", "jsx", "ts", "tsx", "md", "mdx"];
/// Cap on config files read: `routes.ts` and its imports.
const MAX_CONFIG_FILES: usize = 64;

/// The files React Router loads in one app. Paths are canonical.
#[derive(Debug, Default)]
pub struct App {
    pub root: Option<PathBuf>,
    pub route_config: Option<PathBuf>,
    pub client_entry: Option<PathBuf>,
    pub server_entry: Option<PathBuf>,
    /// Every route module, `root` included.
    pub route_modules: FxHashSet<PathBuf>,
}

impl App {
    /// Reads the app of the package in `package_dir`.
    pub fn read(package_dir: &Path) -> Self {
        let app_dir = package_dir.join(app_directory(package_dir));
        let convention_file = |stem: &str| {
            EXTENSIONS
                .iter()
                .find_map(|ext| canonical(&app_dir.join(format!("{stem}.{ext}"))))
        };
        let mut app = Self {
            root: convention_file("root"),
            route_config: convention_file("routes"),
            client_entry: convention_file("entry.client"),
            server_entry: convention_file("entry.server"),
            route_modules: FxHashSet::default(),
        };
        if let Some(route_config) = &app.route_config {
            app.route_modules = route_modules(&app_dir, route_config);
        }
        app.route_modules.extend(app.root.clone());
        app
    }

    /// True when React Router loads `path`, a canonical path.
    pub fn loads(&self, path: &Path) -> bool {
        self.route_modules.contains(path)
            || [&self.route_config, &self.client_entry, &self.server_entry]
                .into_iter()
                .any(|file| file.as_deref() == Some(path))
    }
}

/// The `appDirectory` of the `react-router.config.*` in `package_dir`.
/// `app` by default.
fn app_directory(package_dir: &Path) -> String {
    EXTENSIONS
        .iter()
        .map(|ext| package_dir.join(format!("react-router.config.{ext}")))
        .find_map(|path| Some((std::fs::read_to_string(&path).ok()?, path)))
        .and_then(|(source, path)| {
            with_oxc_parse(&source, &path, |semantic| {
                semantic.nodes().iter().find_map(|node| {
                    let AstKind::ObjectProperty(property) = node.kind() else {
                        return None;
                    };
                    let Expression::StringLiteral(dir) = &property.value else {
                        return None;
                    };
                    (property.key.static_name()? == "appDirectory").then(|| dir.value.to_string())
                })
            })
        })
        .unwrap_or_else(|| "app".to_owned())
}

/// The route modules the route config `routes_file` names.
/// Its config imports by a relative path are read too.
/// File paths are relative to `app_dir`.
fn route_modules(app_dir: &Path, routes_file: &Path) -> FxHashSet<PathBuf> {
    let mut modules = FxHashSet::default();
    let mut read = FxHashSet::default();
    let mut pending = vec![routes_file.to_path_buf()];
    while let Some(file) = pending.pop() {
        if read.len() == MAX_CONFIG_FILES || !read.insert(file.clone()) {
            continue;
        }
        // A file that cannot be read names no route module.
        let config = std::fs::read_to_string(&file)
            .map(|source| RouteConfig::parse(&source, &file))
            .unwrap_or_default();
        modules.extend(
            config
                .files
                .iter()
                .filter_map(|name| canonical(&app_dir.join(name))),
        );
        for flat in &config.flat_routes {
            modules.extend(flat.route_modules(app_dir));
        }
        let dir = file.parent().unwrap_or(app_dir);
        pending.extend(
            config
                .imports
                .iter()
                .filter_map(|specifier| resolve_import(dir, specifier)),
        );
    }
    modules
}

/// What one route config file says.
#[derive(Default)]
struct RouteConfig {
    /// The file argument of each `route()`, `index()` and `layout()` call.
    files: Vec<String>,
    flat_routes: Vec<FlatRoutes>,
    /// Relative specifiers of its imports.
    imports: Vec<String>,
}

/// A route config helper, known by its local import name.
enum Helper {
    /// Takes the route module file at this argument position.
    File(usize),
    FlatRoutes,
}

impl Helper {
    /// The helper `imported` names in the module `from`.
    fn named(from: &str, imported: &str) -> Option<Self> {
        match (from, imported) {
            ("@react-router/dev/routes", "route") => Some(Self::File(1)),
            ("@react-router/dev/routes", "index" | "layout") => Some(Self::File(0)),
            ("@react-router/fs-routes", "flatRoutes") => Some(Self::FlatRoutes),
            _ => None,
        }
    }
}

impl RouteConfig {
    fn parse(source: &str, path: &Path) -> Self {
        with_oxc_parse(source, path, |semantic| {
            let mut config = Self::default();
            let mut helpers = FxHashMap::default();
            for node in semantic.nodes().iter() {
                let AstKind::ImportDeclaration(import) = node.kind() else {
                    continue;
                };
                let from = import.source.value.as_str();
                if from.starts_with("./") || from.starts_with("../") {
                    config.imports.push(from.to_owned());
                }
                for specifier in import.specifiers.iter().flatten() {
                    if let ImportDeclarationSpecifier::ImportSpecifier(specifier) = specifier
                        && let Some(helper) =
                            Helper::named(from, specifier.imported.name().as_str())
                    {
                        helpers.insert(specifier.local.name.as_str(), helper);
                    }
                }
            }
            for node in semantic.nodes().iter() {
                let AstKind::CallExpression(call) = node.kind() else {
                    continue;
                };
                let helper = match &call.callee {
                    Expression::Identifier(callee) => helpers.get(callee.name.as_str()),
                    _ => None,
                };
                match helper {
                    Some(Helper::File(position)) => {
                        if let Some(Argument::StringLiteral(file)) = call.arguments.get(*position) {
                            config.files.push(file.value.to_string());
                        }
                    }
                    Some(Helper::FlatRoutes) => {
                        config
                            .flat_routes
                            .extend(FlatRoutes::from_options(call.arguments.first()));
                    }
                    None => {}
                }
            }
            config
        })
    }
}

/// A `flatRoutes()` call of `@react-router/fs-routes`.
struct FlatRoutes {
    /// Relative to the app directory.
    root_directory: String,
    ignored_route_files: Vec<String>,
}

impl FlatRoutes {
    /// The options of a `flatRoutes()` call, read from literals only.
    /// An unread form may move the directory or skip files.
    /// Then `None`: the call names no route.
    fn from_options(options: Option<&Argument>) -> Option<Self> {
        let mut flat = Self {
            root_directory: "routes".to_owned(),
            ignored_route_files: Vec::new(),
        };
        let Some(options) = options else {
            return Some(flat);
        };
        let Argument::ObjectExpression(options) = options else {
            return None;
        };
        for property in &options.properties {
            let ObjectPropertyKind::ObjectProperty(property) = property else {
                return None;
            };
            match (property.key.static_name().as_deref(), &property.value) {
                (Some("rootDirectory"), Expression::StringLiteral(dir)) => {
                    flat.root_directory = dir.value.to_string();
                }
                (Some("ignoredRouteFiles"), Expression::ArrayExpression(globs)) => {
                    flat.ignored_route_files = globs
                        .elements
                        .iter()
                        .map(|glob| match glob {
                            ArrayExpressionElement::StringLiteral(glob) => {
                                Some(glob.value.to_string())
                            }
                            _ => None,
                        })
                        .collect::<Option<_>>()?;
                }
                (Some("rootDirectory" | "ignoredRouteFiles") | None, _) => return None,
                _ => {}
            }
        }
        Some(flat)
    }

    /// Each file in the routes directory, and each folder's module.
    /// A folder's module is `route`, else `index`.
    /// Dot and `ignoredRouteFiles` globs skip entries, as in React Router.
    fn route_modules(&self, app_dir: &Path) -> Vec<PathBuf> {
        let mut ignored = GlobSetBuilder::new();
        for glob in
            std::iter::once("**/.*").chain(self.ignored_route_files.iter().map(String::as_str))
        {
            // React Router reads these globs with minimatch.
            // Past globset, the skipped files are unknown: add no route.
            if is_minimatch_only(glob) {
                return Vec::new();
            }
            let Ok(glob) = GlobBuilder::new(glob).literal_separator(true).build() else {
                return Vec::new();
            };
            ignored.add(glob);
        }
        let Ok(ignored) = ignored.build() else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(app_dir.join(&self.root_directory)) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.strip_prefix(app_dir)
                    .is_ok_and(|relative| !ignored.is_match(relative))
            })
            .filter_map(|path| {
                if !path.is_dir() {
                    return canonical(&path);
                }
                ["route", "index"].iter().find_map(|stem| {
                    FOLDER_MODULE_EXTENSIONS
                        .iter()
                        .find_map(|ext| canonical(&path.join(format!("{stem}.{ext}"))))
                })
            })
            .collect()
    }
}

/// True if minimatch and globset read `glob` differently.
/// Such is an extglob like `@(a|b)`, or a leading `!`.
fn is_minimatch_only(glob: &str) -> bool {
    glob.starts_with('!')
        || ["@(", "!(", "+(", "*(", "?("]
            .iter()
            .any(|open| glob.contains(open))
}

/// The file a relative import `specifier` in `dir` names.
/// The path itself, then with an extension, then its `index`.
fn resolve_import(dir: &Path, specifier: &str) -> Option<PathBuf> {
    let base = dir.join(specifier);
    let with_extension = EXTENSIONS.iter().map(|ext| {
        let mut name = base.clone().into_os_string();
        name.push(format!(".{ext}"));
        PathBuf::from(name)
    });
    std::iter::once(base.clone())
        .chain(with_extension)
        .chain(
            EXTENSIONS
                .iter()
                .map(|ext| base.join(format!("index.{ext}"))),
        )
        .find(|path| path.is_file())
        .and_then(|path| canonical(&path))
}

fn canonical(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path)
        .ok()
        .filter(|path| path.is_file())
}
