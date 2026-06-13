//! security-detect-non-literal-fs-filename oxc backend.
//!
//! Flags non-literal paths passed to `fs.*` because they are a path-traversal
//! vector when the input is user-controlled. A path is *not* flagged when it is
//! provably derived from a module-root source joined with only literal
//! segments — path traversal is impossible in that case.
//!
//! Safe path sources recognised:
//! - `__dirname` / `__filename`
//! - `import.meta.dirname` / `import.meta.filename` / `import.meta.url`
//! - `fileURLToPath(<safe>)` / `dirname(<safe>)`
//! - `path.dirname(...)` / `path.resolve(...)` / `path.join(...)` whose root
//!   argument is a safe source and every other argument is a string literal
//! - a `const` local bound to any of the above (followed through the file)
//! - a parameter whose every call site in the file passes a safe value
//!   (including self-recursive `fn(path.join(param, lit))`)
//!
//! When safety cannot be proven the rule keeps firing — false negatives on a
//! security rule are preferred over over-exempting genuinely dynamic paths.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BindingPattern, CallExpression, Expression};
use oxc_span::GetSpan;
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

/// fs.* methods that take a path as first argument.
const FS_PATH_METHODS: &[&str] = &[
    "readFile",
    "readFileSync",
    "writeFile",
    "writeFileSync",
    "appendFile",
    "appendFileSync",
    "open",
    "openSync",
    "rm",
    "rmSync",
    "unlink",
    "unlinkSync",
    "stat",
    "statSync",
    "lstat",
    "lstatSync",
    "access",
    "accessSync",
    "createReadStream",
    "createWriteStream",
    "readdir",
    "readdirSync",
    "mkdir",
    "mkdirSync",
    "rmdir",
    "rmdirSync",
    "copyFile",
    "copyFileSync",
    "rename",
    "renameSync",
    "exists",
    "existsSync",
];

/// `path.*` methods that produce a path from a root + segments.
const PATH_JOIN_METHODS: &[&str] = &["join", "resolve", "dirname"];

fn callee_uses_fs(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let method = member.property.name.as_str();
    if !FS_PATH_METHODS.contains(&method) {
        return false;
    }
    let receiver_name = match &member.object {
        Expression::Identifier(id) => id.name.as_str(),
        _ => return false,
    };
    receiver_name == "fs" || receiver_name == "fsPromises" || receiver_name == "fsp"
}

fn is_string_literal(expr: &Expression) -> bool {
    match expr {
        Expression::StringLiteral(_) => true,
        Expression::TemplateLiteral(tpl) => tpl.expressions.is_empty(),
        _ => false,
    }
}

/// `import.meta.<prop>` where prop is one of the location members.
fn is_import_meta_location(expr: &Expression, source: &str) -> bool {
    let Expression::StaticMemberExpression(member) = expr else {
        return false;
    };
    if !matches!(member.property.name.as_str(), "dirname" | "filename" | "url") {
        return false;
    }
    if matches!(&member.object, Expression::MetaProperty(_)) {
        return true;
    }
    let obj = &source[member.object.span().start as usize..member.object.span().end as usize];
    obj == "import.meta"
}

/// Bare `__dirname` / `__filename` module globals.
fn is_dir_global(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(id) if matches!(id.name.as_str(), "__dirname" | "__filename"))
}

/// Resolves the callee to a `path.<method>` member or a bare `dirname` /
/// `fileURLToPath` / `resolve` / `join` identifier, returning the method name.
fn path_helper_method<'a>(call: &'a CallExpression<'a>, source: &str) -> Option<&'a str> {
    match &call.callee {
        Expression::StaticMemberExpression(member) => {
            let obj = &source[member.object.span().start as usize..member.object.span().end as usize];
            if obj == "path" {
                Some(member.property.name.as_str())
            } else {
                None
            }
        }
        Expression::Identifier(id) => Some(id.name.as_str()),
        _ => None,
    }
}

/// A safe path source: a module-root location, or a `path.*` / `fileURLToPath`
/// derivation whose root is itself safe and whose extra segments are all
/// literals. `safe_consts` lets a bound `const` count as a safe root.
fn is_safe_source(expr: &Expression, source: &str, safe_consts: &FxHashSet<String>) -> bool {
    if is_dir_global(expr) || is_import_meta_location(expr, source) {
        return true;
    }
    if let Expression::Identifier(id) = expr {
        return safe_consts.contains(id.name.as_str());
    }
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Some(method) = path_helper_method(call, source) else {
        return false;
    };
    match method {
        "fileURLToPath" => {
            // fileURLToPath(<safe>) — single arg must be a safe source.
            single_arg(call).is_some_and(|a| is_safe_source(a, source, safe_consts))
        }
        m if PATH_JOIN_METHODS.contains(&m) => {
            // A spread argument defeats the literal-segment guarantee.
            if call.arguments.iter().any(|a| matches!(a, Argument::SpreadElement(_))) {
                return false;
            }
            // First arg is the root (must be safe); every other arg a literal.
            let mut args = call.arguments.iter();
            let Some(root) = args.next().and_then(Argument::as_expression) else {
                return false;
            };
            if !is_safe_source(root, source, safe_consts) {
                return false;
            }
            args.all(|a| a.as_expression().is_some_and(is_string_literal))
        }
        _ => false,
    }
}

fn single_arg<'a>(call: &'a CallExpression<'a>) -> Option<&'a Expression<'a>> {
    if call.arguments.len() != 1 {
        return None;
    }
    call.arguments[0].as_expression()
}

/// Collect names of `const` bindings whose initializer is a safe path source.
/// Iterates to a fixpoint so a `const` referencing an earlier safe `const`
/// (declared in any order) is itself recognised as safe.
fn collect_safe_consts<'a>(
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> FxHashSet<String> {
    let mut safe: FxHashSet<String> = FxHashSet::default();
    loop {
        let mut added = false;
        for node in semantic.nodes().iter() {
            let AstKind::VariableDeclaration(decl) = node.kind() else {
                continue;
            };
            if !decl.kind.is_const() {
                continue;
            }
            for declarator in &decl.declarations {
                let BindingPattern::BindingIdentifier(ident) = &declarator.id else {
                    continue;
                };
                let name = ident.name.as_str();
                if safe.contains(name) {
                    continue;
                }
                let Some(init) = &declarator.init else {
                    continue;
                };
                if is_safe_source(init, source, &safe) {
                    safe.insert(name.to_string());
                    added = true;
                }
            }
        }
        if !added {
            return safe;
        }
    }
}

/// The argument passed at `param_index` of a call to `fn_name` is safe if it is
/// a safe source or a `path.join`/`path.resolve` rooted at `param_name` with
/// only literal segments (self-recursive safe propagation).
fn is_safe_arg_for_param(
    arg: &Expression,
    param_name: &str,
    source: &str,
    safe_consts: &FxHashSet<String>,
) -> bool {
    if is_safe_source(arg, source, safe_consts) {
        return true;
    }
    let Expression::CallExpression(call) = arg else {
        return false;
    };
    let Some(method) = path_helper_method(call, source) else {
        return false;
    };
    if !PATH_JOIN_METHODS.contains(&method) {
        return false;
    }
    if call.arguments.iter().any(|a| matches!(a, Argument::SpreadElement(_))) {
        return false;
    }
    let mut args = call.arguments.iter();
    let Some(root) = args.next().and_then(Argument::as_expression) else {
        return false;
    };
    let root_is_param =
        matches!(root, Expression::Identifier(id) if id.name.as_str() == param_name);
    if !root_is_param && !is_safe_source(root, source, safe_consts) {
        return false;
    }
    args.all(|a| a.as_expression().is_some_and(is_string_literal))
}

/// A parameter is safe when it belongs to a *named* function and every call to
/// that function in the file passes a safe value at the parameter's position.
/// Anonymous functions and parameters with no call sites keep firing (sound).
fn is_safe_parameter<'a>(
    name: &str,
    node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
    safe_consts: &FxHashSet<String>,
) -> bool {
    let nodes = semantic.nodes();

    // Find the enclosing named function and the parameter's position.
    let mut fn_name: Option<&str> = None;
    let mut param_index: Option<usize> = None;
    for ancestor in nodes.ancestors(node_id) {
        let AstKind::Function(func) = ancestor.kind() else {
            continue;
        };
        // Anonymous function: cannot enumerate call sites → keep firing.
        let Some(id) = func.id.as_ref() else {
            return false;
        };
        for (idx, param) in func.params.items.iter().enumerate() {
            if let BindingPattern::BindingIdentifier(p) = &param.pattern
                && p.name.as_str() == name
            {
                fn_name = Some(id.name.as_str());
                param_index = Some(idx);
                break;
            }
        }
        break;
    }
    let (Some(fn_name), Some(param_index)) = (fn_name, param_index) else {
        return false;
    };

    // Every call to this function in the file must pass a safe value here.
    let mut saw_call = false;
    for n in nodes.iter() {
        let AstKind::CallExpression(call) = n.kind() else {
            continue;
        };
        let Expression::Identifier(callee) = &call.callee else {
            continue;
        };
        if callee.name.as_str() != fn_name {
            continue;
        }
        saw_call = true;
        let Some(arg) = call.arguments.get(param_index).and_then(Argument::as_expression) else {
            return false;
        };
        if !is_safe_arg_for_param(arg, name, source, safe_consts) {
            return false;
        }
    }
    saw_call
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[] // full-program analysis: needs cross-node binding/data-flow
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["fs."])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let source = ctx.source;
        let safe_consts = collect_safe_consts(semantic, source);
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };
            if !callee_uses_fs(call) {
                continue;
            }
            let Some(first_arg) = call.arguments.first().and_then(Argument::as_expression) else {
                continue;
            };
            if is_string_literal(first_arg) || is_safe_source(first_arg, source, &safe_consts) {
                continue;
            }
            if let Expression::Identifier(id) = first_arg
                && is_safe_parameter(id.name.as_str(), node.id(), semantic, source, &safe_consts)
            {
                continue;
            }
            let (line, column) = byte_offset_to_line_col(source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Dynamic path passed to `fs.*` — path traversal vector when the \
                          input is user-controlled. Validate against an allowlist."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_fs_read_dynamic() {
        let src = r#"const r = fs.readFileSync(userInput);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_fs_read_literal() {
        let src = r#"const r = fs.readFileSync("config.json");"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_dirname_derived_const_passed_to_function() {
        // Regression for #1116: the path is provably derived from a module-root
        // source (import.meta.url) joined with only literal segments.
        let src = r#"
import path from "node:path";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
function copyAssetsSync(source: string, dest: string) {
  const stat = fs.statSync(source);
  for (const item of fs.readdirSync(source))
    copyAssetsSync(path.join(source, item), path.join(dest, item));
}
const source = path.join(__dirname, "..", "..", "theme", "assets");
const dest = path.join(__dirname, "out");
copyAssetsSync(source, dest);
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_dirname_global_joined_with_literals() {
        let src = r#"
const p = path.join(__dirname, "data", "config.json");
const r = fs.readFileSync(p);
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_import_meta_dirname_resolved() {
        let src = r#"
const r = fs.readFileSync(path.resolve(import.meta.dirname, "fixtures", "a.txt"));
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_user_query_path() {
        let src = r#"const r = fs.readFileSync(req.query.path);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_user_input_concatenation() {
        let src = r#"const r = fs.readFileSync(userInput + ".txt");"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_join_with_non_literal_segment() {
        // Root is __dirname, but a segment is a dynamic identifier → traversal possible.
        let src = r#"const r = fs.readFileSync(path.join(__dirname, userInput));"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_join_from_non_safe_variable() {
        // `base` is initialized from a non-safe source, so the join is not safe.
        let src = r#"
const base = req.query.dir;
const p = path.join(base, "x.txt");
const r = fs.readFileSync(p);
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_parameter_with_unsafe_call_site() {
        // The function is called once with a user-controlled path.
        let src = r#"
function read(p: string) {
  return fs.readFileSync(p);
}
read(req.query.path);
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_parameter_with_no_call_site() {
        // Exported helper with no in-file call site: cannot prove safety.
        let src = r#"
export function read(p: string) {
  return fs.readFileSync(p);
}
"#;
        assert_eq!(run(src).len(), 1);
    }
}
