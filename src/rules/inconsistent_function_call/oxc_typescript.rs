//! inconsistent-function-call oxc backend.
//!
//! Collects every `function_declaration` in the file, then scans all call
//! sites for each name. If a function is called both as `new Foo(...)` and
//! `Foo(...)`, emit one diagnostic per inconsistent call site.

use rustc_hash::FxHashMap;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::import_index::CallKind;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;

pub struct Check;

/// Where a function was declared and whether it is exported.
#[derive(Debug, Clone)]
struct DeclInfo {
    line: usize,
    exported: bool,
    /// `true` when the body opens with the new-optional self-redirect guard
    /// (`if (!(this instanceof F)) return new F(...)`), so bare `F(...)` calls
    /// transparently become `new F(...)`. Both call styles are then correct.
    new_optional: bool,
}

/// A call or `new` site.
#[derive(Debug, Clone)]
struct Site {
    path: std::path::PathBuf,
    line: usize,
    column: usize,
    byte_offset: usize,
    byte_len: usize,
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let program = semantic.nodes().program();

        // 1. Collect every function_declaration name + whether it is exported.
        let mut declared: FxHashMap<String, DeclInfo> = FxHashMap::default();
        collect_function_declarations(program, ctx.source, &mut declared);
        if declared.is_empty() {
            return Vec::new();
        }

        // 2. Scan every call site in THIS file.
        let declared_names: Vec<String> = declared.keys().cloned().collect();
        let mut new_sites: FxHashMap<String, Vec<Site>> = FxHashMap::default();
        let mut plain_sites: FxHashMap<String, Vec<Site>> = FxHashMap::default();

        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::NewExpression(new_expr) => {
                    if let Expression::Identifier(callee) = &new_expr.callee {
                        let name = callee.name.as_str();
                        if declared_names.iter().any(|n| n == name) {
                            let span = new_expr.span;
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, span.start as usize);
                            new_sites
                                .entry(name.to_string())
                                .or_default()
                                .push(Site {
                                    path: ctx.path.to_path_buf(),
                                    line,
                                    column,
                                    byte_offset: span.start as usize,
                                    byte_len: (span.end - span.start) as usize,
                                });
                        }
                    }
                }
                AstKind::CallExpression(call_expr) => {
                    if let Expression::Identifier(callee) = &call_expr.callee {
                        let name = callee.name.as_str();
                        if declared_names.iter().any(|n| n == name) {
                            let span = call_expr.span;
                            let (line, column) =
                                byte_offset_to_line_col(ctx.source, span.start as usize);
                            plain_sites
                                .entry(name.to_string())
                                .or_default()
                                .push(Site {
                                    path: ctx.path.to_path_buf(),
                                    line,
                                    column,
                                    byte_offset: span.start as usize,
                                    byte_len: (span.end - span.start) as usize,
                                });
                        }
                    }
                }
                _ => {}
            }
        }

        // 3. Merge in cross-file call sites for exported functions.
        let index = ctx.project.import_index();
        for (name, info) in &declared {
            if !info.exported {
                continue;
            }
            for site in index.get_call_sites(ctx.path, name) {
                let bucket = match site.kind {
                    CallKind::New => new_sites.entry(name.clone()).or_default(),
                    CallKind::Call => plain_sites.entry(name.clone()).or_default(),
                };
                bucket.push(Site {
                    path: site.path.clone(),
                    line: site.line,
                    column: site.column,
                    byte_offset: site.byte_offset,
                    byte_len: site.byte_len,
                });
            }
        }

        // 4. For every function called in BOTH styles, emit a diagnostic on
        //    every call site.
        let mut diagnostics = Vec::new();
        for (name, info) in &declared {
            // New-optional constructors (`if (!(this instanceof F)) return new
            // F(...)`) deliberately support both `F(...)` and `new F(...)`.
            if info.new_optional {
                continue;
            }
            let news = new_sites.get(name);
            let plains = plain_sites.get(name);
            let (Some(news), Some(plains)) = (news, plains) else {
                continue;
            };
            if news.is_empty() || plains.is_empty() {
                continue;
            }

            let decl_line = info.line;
            let decl_path = ctx.path.display().to_string();
            for site in news.iter().chain(plains.iter()) {
                diagnostics.push(Diagnostic {
                    path: site.path.clone().into(),
                    line: site.line,
                    column: site.column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Function `{name}` (declared in {decl_path}:{decl_line}) is called both with and without `new`. Pick one style — use `new` for constructors, never for plain functions."
                    ),
                    severity: Severity::Error,
                    span: Some((site.byte_offset, site.byte_len)),
                });
            }
        }

        diagnostics
    }
}

/// Walk the program body and record every `function_declaration` name.
/// A declaration is marked `exported` when it appears inside an export.
/// Nested declarations count too (walked recursively via statements).
fn collect_function_declarations(
    program: &Program<'_>,
    source: &str,
    out: &mut FxHashMap<String, DeclInfo>,
) {
    for stmt in &program.body {
        collect_from_statement(stmt, source, false, out);
    }
}

fn collect_from_statement(
    stmt: &Statement<'_>,
    source: &str,
    exported: bool,
    out: &mut FxHashMap<String, DeclInfo>,
) {
    match stmt {
        Statement::FunctionDeclaration(f) => {
            if let Some(ref id) = f.id {
                let (line, _) = byte_offset_to_line_col(source, f.span.start as usize);
                out.entry(id.name.to_string()).or_insert(DeclInfo {
                    line,
                    exported,
                    new_optional: has_new_optional_guard(f, id.name.as_str()),
                });
            }
        }
        Statement::ExportNamedDeclaration(export) => {
            if let Some(ref decl) = export.declaration {
                collect_from_declaration(decl, source, true, out);
            }
        }
        Statement::ExportDefaultDeclaration(export) => {
            if let ExportDefaultDeclarationKind::FunctionDeclaration(f) = &export.declaration
                && let Some(ref id) = f.id {
                    let (line, _) = byte_offset_to_line_col(source, f.span.start as usize);
                    out.entry(id.name.to_string()).or_insert(DeclInfo {
                        line,
                        exported: true,
                        new_optional: has_new_optional_guard(f, id.name.as_str()),
                    });
                }
        }
        Statement::BlockStatement(block) => {
            for s in &block.body {
                collect_from_statement(s, source, false, out);
            }
        }
        _ => {}
    }
}

fn collect_from_declaration(
    decl: &Declaration<'_>,
    source: &str,
    exported: bool,
    out: &mut FxHashMap<String, DeclInfo>,
) {
    if let Declaration::FunctionDeclaration(f) = decl
        && let Some(ref id) = f.id {
            let (line, _) = byte_offset_to_line_col(source, f.span.start as usize);
            out.entry(id.name.to_string()).or_insert(DeclInfo {
                line,
                exported,
                new_optional: has_new_optional_guard(f, id.name.as_str()),
            });
        }
}

/// Detect the new-optional self-redirect guard at the top of a function body:
///
/// ```js
/// function F(a) {
///   if (!(this instanceof F)) return new F(a);  // or `this instanceof F === false`
///   // ...
/// }
/// ```
///
/// `this` may be aliased to a local (`let that = this; if (!(that instanceof F))`),
/// which is the form used by currency.js. The guard is matched anywhere among the
/// function's top-level statements, so an unrelated leading `if` does not hide it.
/// When the guard is present, a bare `F(...)` call self-redirects to `new F(...)`,
/// so both call styles are correct.
fn has_new_optional_guard(func: &Function<'_>, name: &str) -> bool {
    let Some(body) = func.body.as_deref() else {
        return false;
    };

    // Locals bound to `this` before the guard (`let that = this`). `this` itself
    // always counts as a valid left operand of the `instanceof` check.
    let mut this_aliases: Vec<&str> = Vec::new();

    for stmt in &body.statements {
        match stmt {
            Statement::VariableDeclaration(decl) => {
                for declarator in &decl.declarations {
                    if let (Some(Expression::ThisExpression(_)), BindingPattern::BindingIdentifier(id)) =
                        (&declarator.init, &declarator.id)
                    {
                        this_aliases.push(id.name.as_str());
                    }
                }
            }
            Statement::IfStatement(if_stmt) => {
                if is_self_redirect_guard(if_stmt, name, &this_aliases) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// `if (!(<this> instanceof F)) return new F(...)` — test is a negated
/// `instanceof` against `F`, consequent returns `new F(...)`.
fn is_self_redirect_guard(if_stmt: &IfStatement<'_>, name: &str, this_aliases: &[&str]) -> bool {
    is_negated_instanceof(&if_stmt.test, name, this_aliases)
        && consequent_returns_new(&if_stmt.consequent, name)
}

/// `!(<this> instanceof F)` or `(<this> instanceof F) === false`.
fn is_negated_instanceof(test: &Expression<'_>, name: &str, this_aliases: &[&str]) -> bool {
    match test {
        Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
            is_this_instanceof(&unary.argument, name, this_aliases)
        }
        Expression::BinaryExpression(bin)
            if matches!(
                bin.operator,
                BinaryOperator::StrictEquality | BinaryOperator::Equality
            ) =>
        {
            (is_this_instanceof(&bin.left, name, this_aliases) && is_false_literal(&bin.right))
                || (is_this_instanceof(&bin.right, name, this_aliases) && is_false_literal(&bin.left))
        }
        Expression::ParenthesizedExpression(paren) => {
            is_negated_instanceof(&paren.expression, name, this_aliases)
        }
        _ => false,
    }
}

/// `<this> instanceof F`, where `<this>` is `this` or a local aliased to it.
fn is_this_instanceof(expr: &Expression<'_>, name: &str, this_aliases: &[&str]) -> bool {
    match expr {
        Expression::ParenthesizedExpression(paren) => {
            is_this_instanceof(&paren.expression, name, this_aliases)
        }
        Expression::BinaryExpression(bin) if bin.operator == BinaryOperator::Instanceof => {
            let left_is_this = match &bin.left {
                Expression::ThisExpression(_) => true,
                Expression::Identifier(id) => this_aliases.contains(&id.name.as_str()),
                _ => false,
            };
            let right_is_fn = matches!(
                &bin.right,
                Expression::Identifier(id) if id.name.as_str() == name
            );
            left_is_this && right_is_fn
        }
        _ => false,
    }
}

fn is_false_literal(expr: &Expression<'_>) -> bool {
    matches!(expr, Expression::BooleanLiteral(lit) if !lit.value)
}

/// The guard's consequent (`return new F(...)`, optionally wrapped in a block)
/// returns a `new F(...)`.
fn consequent_returns_new(consequent: &Statement<'_>, name: &str) -> bool {
    match consequent {
        Statement::ReturnStatement(ret) => ret
            .argument
            .as_ref()
            .is_some_and(|arg| is_new_of(arg, name)),
        Statement::BlockStatement(block) => block
            .body
            .iter()
            .any(|s| consequent_returns_new(s, name)),
        _ => false,
    }
}

/// `new F(...)`.
fn is_new_of(expr: &Expression<'_>, name: &str) -> bool {
    matches!(
        expr,
        Expression::NewExpression(new_expr)
            if matches!(&new_expr.callee, Expression::Identifier(id) if id.name.as_str() == name)
    )
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.js")
    }

    #[test]
    fn new_optional_guard_with_this_alias_is_exempt() {
        // #5956 — currency.js: `this` is aliased to `that`, the guard redirects
        // bare calls to `new`, and internal methods call the function without
        // `new`. Both styles are correct → no diagnostic.
        let src = r#"
            function currency(value, opts) {
              let that = this;
              if (!(that instanceof currency)) {
                return new currency(value, opts);
              }
              this.value = value;
            }
            const a = new currency(1);
            function add() { return currency(2); }
        "#;
        assert!(run_on(src).is_empty(), "new-optional ctor must not be flagged");
    }

    #[test]
    fn new_optional_guard_with_plain_this_is_exempt() {
        let src = r#"
            function Money(v) {
              if (!(this instanceof Money)) return new Money(v);
              this.v = v;
            }
            const a = new Money(1);
            function make() { return Money(2); }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn new_optional_guard_via_equals_false_is_exempt() {
        let src = r#"
            function Money(v) {
              if (this instanceof Money === false) return new Money(v);
              this.v = v;
            }
            const a = new Money(1);
            function make() { return Money(2); }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn guard_after_unrelated_leading_if_is_exempt() {
        // A benign argument-check `if` precedes the self-redirect guard.
        let src = r#"
            function Money(v) {
              if (v == null) v = 0;
              if (!(this instanceof Money)) return new Money(v);
              this.v = v;
            }
            const a = new Money(1);
            function make() { return Money(2); }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn inconsistent_call_without_guard_still_flags() {
        // No instanceof self-redirect guard → genuinely inconsistent.
        let src = r#"
            function Widget(v) { this.v = v; }
            const a = new Widget(1);
            function make() { return Widget(2); }
        "#;
        let d = run_on(src);
        assert_eq!(d.len(), 2, "both call sites flagged when no guard");
        assert!(d.iter().all(|x| x.rule_id == "inconsistent-function-call"));
    }

    #[test]
    fn guard_against_other_name_still_flags() {
        // The instanceof operand must be the function's OWN name. Here the guard
        // checks `Other`, not `Widget`, so Widget is still inconsistent.
        let src = r#"
            function Widget(v) {
              if (!(this instanceof Other)) return new Other(v);
              this.v = v;
            }
            const a = new Widget(1);
            function make() { return Widget(2); }
        "#;
        let d = run_on(src);
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn consistent_new_only_is_not_flagged() {
        let src = r#"
            function Widget(v) { this.v = v; }
            const a = new Widget(1);
            const b = new Widget(2);
        "#;
        assert!(run_on(src).is_empty());
    }
}
