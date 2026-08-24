//! no-single-use-one-liner oxc backend — flag a function whose body is one
//! expression, whose name no other module can reach, and whose name is read
//! exactly once in the file, at a call.
//!
//! Three declaration shapes carry such a name: a function declaration, a `const`
//! bound to an arrow or function expression, and a private class method. The
//! first two resolve their readers through the symbol table; a private method
//! has no symbol, so its readers are the `this.<name>` reads inside its own
//! class, which is every place it can be reached from.
//!
//! A name read anywhere but as a callee — passed to `map`, stored in an object,
//! exported by a later `export { … }` — is not a single-call helper: the value
//! travels, so no call site can absorb it. The same holds for a name read twice.
//!
//! Exempt whatever the body: an exported function (its callers live in other
//! files), a hook (`use…`, whose identity drives the rules of hooks), a React
//! component returning JSX (a name the renderer needs), an overload
//! implementation (inlining erases the signatures), a generator, a type-guard
//! (`x is Y`, a name the narrowing needs), and any name matching `ignore_names`.
//!
//! A body over `max_body_tokens` is exempt too: past that width the call site
//! reads worse with the expression pasted into it than with the name.
//!
//! A method body that forwards its own parameters to another `this` method
//! belongs to `no-shallow-passthrough-method` — one span, one owner.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, span_contains};
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use globset::{Glob, GlobMatcher};
use oxc_ast::ast::*;
use oxc_semantic::{Semantic, SymbolId};
use oxc_span::{GetSpan, Span};
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

/// A `this.<name>` or `this.#<name>` read, and whether it is the callee of a
/// call. Collected once per file because a private method resolves its readers
/// by name, not through the symbol table.
struct ThisRead<'a> {
    name: &'a str,
    is_private_field: bool,
    span: Span,
    is_callee: bool,
}

/// File-wide facts and the two configured knobs, gathered once per run.
struct Scope<'a> {
    source: &'a str,
    max_body_tokens: usize,
    ignore_names: Vec<GlobMatcher>,
    exported_names: FxHashSet<&'a str>,
    overload_names: FxHashSet<&'a str>,
    this_reads: Vec<ThisRead<'a>>,
}

/// A reportable helper: the name to quote and the byte offset to point at.
struct Finding<'a> {
    name: &'a str,
    offset: u32,
}

/// The single expression a body reduces to — `=> expr`, `{ return expr; }` or
/// `{ expr; }`. `None` for a body that runs more than one thing, or for a bare
/// `return;`, which has nothing to paste.
fn sole_body_expression<'a>(body: &'a FunctionBody<'a>) -> Option<&'a Expression<'a>> {
    if body.statements.len() != 1 {
        return None;
    }
    match &body.statements[0] {
        Statement::ReturnStatement(statement) => statement.argument.as_ref(),
        Statement::ExpressionStatement(statement) => Some(&statement.expression),
        _ => None,
    }
}

/// Lexical token count of `text`: one per identifier or number run, one per
/// string or template literal, one per punctuation character. It measures how
/// wide the paste at the call site would be, so it rounds up on multi-character
/// operators rather than reproducing a TypeScript tokenizer.
fn token_count(text: &str) -> usize {
    let mut count = 0;
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character.is_whitespace() {
            continue;
        }
        if is_word_character(character) {
            while characters.peek().copied().is_some_and(is_word_character) {
                characters.next();
            }
        } else if matches!(character, '"' | '\'' | '`') {
            let mut is_escaped = false;
            for next in characters.by_ref() {
                if is_escaped {
                    is_escaped = false;
                } else if next == '\\' {
                    is_escaped = true;
                } else if next == character {
                    break;
                }
            }
        }
        count += 1;
    }
    count
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_' || character == '$'
}

/// True when the annotation is a type predicate (`value is Cat`). The name is
/// what narrows at the call site, so the function cannot be pasted away.
fn returns_type_predicate(annotation: Option<&TSTypeAnnotation>) -> bool {
    annotation.is_some_and(|annotation| matches!(annotation.type_annotation, TSType::TSTypePredicate(_)))
}

/// True when the expression evaluates to JSX on at least one branch.
fn yields_jsx(expression: &Expression) -> bool {
    match expression {
        Expression::JSXElement(_) | Expression::JSXFragment(_) => true,
        Expression::ParenthesizedExpression(inner) => yields_jsx(&inner.expression),
        Expression::TSAsExpression(inner) => yields_jsx(&inner.expression),
        Expression::TSNonNullExpression(inner) => yields_jsx(&inner.expression),
        Expression::ConditionalExpression(inner) => {
            yields_jsx(&inner.consequent) || yields_jsx(&inner.alternate)
        }
        Expression::LogicalExpression(inner) => yields_jsx(&inner.right),
        _ => false,
    }
}

/// True when `name` follows the React hook convention (`use` then an uppercase
/// letter).
fn is_hook_name(name: &str) -> bool {
    name.strip_prefix("use")
        .and_then(|rest| rest.chars().next())
        .is_some_and(char::is_uppercase)
}

fn is_component_name(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}

/// Names a later `export { … }` clause carries. A declaration named there is
/// reachable from another module even though its own statement has no `export`
/// keyword.
fn export_clause_names<'a>(semantic: &'a Semantic<'a>) -> FxHashSet<&'a str> {
    let mut names = FxHashSet::default();
    for node in semantic.nodes().iter() {
        let AstKind::ExportNamedDeclaration(declaration) = node.kind() else {
            continue;
        };
        for specifier in &declaration.specifiers {
            match &specifier.local {
                ModuleExportName::IdentifierReference(identifier) => {
                    names.insert(identifier.name.as_str());
                }
                ModuleExportName::IdentifierName(identifier) => {
                    names.insert(identifier.name.as_str());
                }
                ModuleExportName::StringLiteral(_) => {}
            }
        }
    }
    names
}

/// Names declared by a bodyless function — a TypeScript overload signature. The
/// bodied declaration under such a name is the implementation of the set.
fn overload_signature_names<'a>(semantic: &'a Semantic<'a>) -> FxHashSet<&'a str> {
    semantic
        .nodes()
        .iter()
        .filter_map(|node| {
            let AstKind::Function(function) = node.kind() else {
                return None;
            };
            if function.body.is_some() {
                return None;
            }
            function.id.as_ref().map(|id| id.name.as_str())
        })
        .collect()
}

/// Every `this.<name>` and `this.#<name>` read in the file.
fn this_member_reads<'a>(semantic: &'a Semantic<'a>) -> Vec<ThisRead<'a>> {
    let nodes = semantic.nodes();
    let mut reads = Vec::new();
    for node in nodes.iter() {
        let (name, is_private_field, span) = match node.kind() {
            AstKind::StaticMemberExpression(member) => {
                if !matches!(member.object, Expression::ThisExpression(_)) {
                    continue;
                }
                (member.property.name.as_str(), false, member.span)
            }
            AstKind::PrivateFieldExpression(member) => {
                if !matches!(member.object, Expression::ThisExpression(_)) {
                    continue;
                }
                (member.field.name.as_str(), true, member.span)
            }
            _ => continue,
        };
        let is_callee = matches!(
            nodes.parent_node(node.id()).kind(),
            AstKind::CallExpression(call) if call.callee.span() == span
        );
        reads.push(ThisRead {
            name,
            is_private_field,
            span,
            is_callee,
        });
    }
    reads
}

/// True when the statement declaring the function carries an `export` keyword.
/// The walk stops at the first node that is not a declaration wrapper, so a
/// helper nested inside an exported function stays local.
fn declared_under_export(semantic: &Semantic, node_id: oxc_semantic::NodeId) -> bool {
    for ancestor in semantic.nodes().ancestors(node_id) {
        match ancestor.kind() {
            AstKind::ExportNamedDeclaration(_) | AstKind::ExportDefaultDeclaration(_) => {
                return true;
            }
            AstKind::VariableDeclaration(_) => {}
            _ => return false,
        }
    }
    false
}

/// True when exactly one call reads `symbol_id` and nothing else does. A read in
/// any other position means the function travels as a value, and a call inside
/// `own_span` is the recursive one, which no call site can absorb.
fn has_sole_outside_call(semantic: &Semantic, symbol_id: SymbolId, own_span: Span) -> bool {
    let nodes = semantic.nodes();
    let mut call_site: Option<Span> = None;
    for reference in semantic.symbol_references(symbol_id) {
        let reference_node_id = reference.node_id();
        let reference_span = nodes.kind(reference_node_id).span();
        let AstKind::CallExpression(call) = nodes.parent_node(reference_node_id).kind() else {
            return false;
        };
        if call.callee.span() != reference_span || call_site.is_some() {
            return false;
        }
        call_site = Some(reference_span);
    }
    call_site.is_some_and(|span| !span_contains(own_span, span))
}

/// True when exactly one call inside `class_span` reads the private member, and
/// nothing else does.
fn has_sole_this_call(
    scope: &Scope,
    name: &str,
    is_private_field: bool,
    class_span: Span,
    own_span: Span,
) -> bool {
    let mut call_site: Option<Span> = None;
    for read in &scope.this_reads {
        if read.name != name
            || read.is_private_field != is_private_field
            || !span_contains(class_span, read.span)
        {
            continue;
        }
        if !read.is_callee || call_site.is_some() {
            return false;
        }
        call_site = Some(read.span);
    }
    call_site.is_some_and(|span| !span_contains(own_span, span))
}

/// True when the expression forwards the parameter list, in order, to another
/// `this` method — the pass-through shape `no-shallow-passthrough-method` owns.
fn forwards_own_parameters(parameters: &FormalParameters, expression: &Expression) -> bool {
    let Expression::CallExpression(call) = expression else {
        return false;
    };
    let forwards_to_this = match &call.callee {
        Expression::StaticMemberExpression(member) => {
            matches!(member.object, Expression::ThisExpression(_))
        }
        Expression::PrivateFieldExpression(member) => {
            matches!(member.object, Expression::ThisExpression(_))
        }
        _ => false,
    };
    if !forwards_to_this || parameters.items.is_empty() || parameters.items.len() != call.arguments.len() {
        return false;
    }
    parameters
        .items
        .iter()
        .zip(&call.arguments)
        .all(|(parameter, argument)| match (&parameter.pattern, argument) {
            (BindingPattern::BindingIdentifier(binding), Argument::Identifier(passed)) => {
                binding.name == passed.name
            }
            _ => false,
        })
}

/// True when the body is a single expression the rule is willing to paste: not a
/// React component's JSX, and no wider than `max_body_tokens`.
fn body_is_inlinable(scope: &Scope, name: &str, expression: &Expression) -> bool {
    if is_component_name(name) && yields_jsx(expression) {
        return false;
    }
    let span = expression.span();
    let text = &scope.source[span.start as usize..span.end as usize];
    token_count(text) <= scope.max_body_tokens
}

/// True when the name is one the rule never reports: a hook, or a user glob.
fn name_is_exempt(scope: &Scope, name: &str) -> bool {
    is_hook_name(name) || scope.ignore_names.iter().any(|glob| glob.is_match(name))
}

/// The finding for `function name() { return expr; }`.
fn function_declaration_finding<'a>(
    function: &'a Function<'a>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a Semantic<'a>,
    scope: &Scope<'a>,
) -> Option<Finding<'a>> {
    if function.generator || returns_type_predicate(function.return_type.as_deref()) {
        return None;
    }
    let id = function.id.as_ref()?;
    let name = id.name.as_str();
    if name_is_exempt(scope, name)
        || scope.overload_names.contains(name)
        || scope.exported_names.contains(name)
        || declared_under_export(semantic, node.id())
    {
        return None;
    }
    let expression = sole_body_expression(function.body.as_deref()?)?;
    if !body_is_inlinable(scope, name, expression) {
        return None;
    }
    let symbol_id = id.symbol_id.get()?;
    if !has_sole_outside_call(semantic, symbol_id, function.span) {
        return None;
    }
    Some(Finding {
        name,
        offset: id.span.start,
    })
}

/// The finding for `const name = () => expr;`.
fn variable_declarator_finding<'a>(
    declarator: &'a VariableDeclarator<'a>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a Semantic<'a>,
    scope: &Scope<'a>,
) -> Option<Finding<'a>> {
    let BindingPattern::BindingIdentifier(id) = &declarator.id else {
        return None;
    };
    let name = id.name.as_str();
    if name_is_exempt(scope, name)
        || scope.exported_names.contains(name)
        || declared_under_export(semantic, node.id())
    {
        return None;
    }
    let body = match declarator.init.as_ref()? {
        Expression::ArrowFunctionExpression(arrow) => {
            if returns_type_predicate(arrow.return_type.as_deref()) {
                return None;
            }
            &arrow.body
        }
        Expression::FunctionExpression(function) => {
            if function.generator || returns_type_predicate(function.return_type.as_deref()) {
                return None;
            }
            function.body.as_ref()?
        }
        _ => return None,
    };
    let expression = sole_body_expression(body)?;
    if !body_is_inlinable(scope, name, expression) {
        return None;
    }
    let symbol_id = id.symbol_id.get()?;
    if !has_sole_outside_call(semantic, symbol_id, declarator.span) {
        return None;
    }
    Some(Finding {
        name,
        offset: id.span.start,
    })
}

/// The finding for a private method, `#name()` or `private name()`.
fn private_method_finding<'a>(
    method: &'a MethodDefinition<'a>,
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a Semantic<'a>,
    scope: &Scope<'a>,
) -> Option<Finding<'a>> {
    if method.kind != MethodDefinitionKind::Method || !method.decorators.is_empty() {
        return None;
    }
    let (name, is_private_field) = match &method.key {
        PropertyKey::PrivateIdentifier(id) => (id.name.as_str(), true),
        PropertyKey::StaticIdentifier(id)
            if method.accessibility == Some(TSAccessibility::Private) =>
        {
            (id.name.as_str(), false)
        }
        _ => return None,
    };
    let function = &method.value;
    if function.generator || returns_type_predicate(function.return_type.as_deref()) {
        return None;
    }
    if name_is_exempt(scope, name) {
        return None;
    }
    let expression = sole_body_expression(function.body.as_deref()?)?;
    if !body_is_inlinable(scope, name, expression)
        || forwards_own_parameters(&function.params, expression)
    {
        return None;
    }
    let class_span = semantic.nodes().ancestors(node.id()).find_map(|ancestor| {
        match ancestor.kind() {
            AstKind::Class(class) => Some(class.span),
            _ => None,
        }
    })?;
    if !has_sole_this_call(scope, name, is_private_field, class_span, method.span) {
        return None;
    }
    Some(Finding {
        name,
        offset: method.key.span().start,
    })
}

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let scope = Scope {
            source: ctx.source,
            max_body_tokens: ctx.config.threshold(super::META.id, "max_body_tokens", ctx.lang),
            ignore_names: ctx
                .config
                .string_list(super::META.id, "ignore_names", ctx.lang)
                .iter()
                .filter_map(|pattern| Glob::new(pattern).ok().map(|glob| glob.compile_matcher()))
                .collect(),
            exported_names: export_clause_names(semantic),
            overload_names: overload_signature_names(semantic),
            this_reads: this_member_reads(semantic),
        };

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            let finding = match node.kind() {
                AstKind::Function(function)
                    if function.r#type == FunctionType::FunctionDeclaration =>
                {
                    function_declaration_finding(function, node, semantic, &scope)
                }
                AstKind::VariableDeclarator(declarator) => {
                    variable_declarator_finding(declarator, node, semantic, &scope)
                }
                AstKind::MethodDefinition(method) => {
                    private_method_finding(method, node, semantic, &scope)
                }
                _ => None,
            };
            let Some(finding) = finding else {
                continue;
            };
            let (line, column) = byte_offset_to_line_col(ctx.source, finding.offset as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{}` wraps one expression and has one call site — paste the expression \
                     there and drop the name, which says nothing the expression does not.",
                    finding.name
                ),
                severity: Severity::Error,
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

    fn run_tsx(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_function_declaration_with_one_caller() {
        let diagnostics = run("function double(n: number) { return n * 2; }\nexport const total = double(21);");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("`double`"));
    }

    #[test]
    fn flags_const_arrow_with_one_caller() {
        assert_eq!(
            run("const unwrap = (r: Res) => r.data;\nexport const value = unwrap(response);").len(),
            1
        );
    }

    #[test]
    fn flags_const_function_expression_with_one_caller() {
        assert_eq!(
            run("const unwrap = function (r: Res) { return r.data; };\nexport const value = unwrap(response);")
                .len(),
            1
        );
    }

    #[test]
    fn flags_expression_bodied_arrow_without_return() {
        assert_eq!(
            run("const notify = () => bus.emit('done');\nexport function finish() { notify(); }").len(),
            1
        );
    }

    #[test]
    fn flags_statement_bodied_void_helper() {
        assert_eq!(
            run("function notify() { bus.emit('done'); }\nexport function finish() { notify(); }").len(),
            1
        );
    }

    #[test]
    fn flags_async_helper() {
        assert_eq!(
            run("const load = async (id: Id) => (await api.get(id)).data;\nexport async function show(id: Id) { return load(id); }")
                .len(),
            1
        );
    }

    #[test]
    fn flags_nested_helper_inside_a_function() {
        assert_eq!(
            run("export function report(rows: Row[]) { const label = (r: Row) => r.name; return rows.map((r) => label(r)); }")
                .len(),
            1
        );
    }

    #[test]
    fn flags_private_method_with_one_caller() {
        assert_eq!(
            run("export class Cart { private subtotal() { return this.lines.length; } total() { return this.subtotal() + 1; } }")
                .len(),
            1
        );
    }

    #[test]
    fn flags_ecmascript_private_method_with_one_caller() {
        assert_eq!(
            run("export class Cart { #subtotal() { return this.lines.length; } total() { return this.#subtotal() + 1; } }")
                .len(),
            1
        );
    }

    #[test]
    fn ignores_exported_function() {
        assert!(run("export function double(n: number) { return n * 2; }\nconst t = double(21);").is_empty());
    }

    #[test]
    fn ignores_function_exported_by_a_later_clause() {
        assert!(
            run("function double(n: number) { return n * 2; }\nconst t = double(21);\nexport { double };")
                .is_empty()
        );
    }

    #[test]
    fn ignores_helper_with_two_call_sites() {
        assert!(
            run("const unwrap = (r: Res) => r.data;\nexport const a = unwrap(x);\nexport const b = unwrap(y);")
                .is_empty()
        );
    }

    #[test]
    fn ignores_helper_with_no_call_site() {
        assert!(run("const unwrap = (r: Res) => r.data;").is_empty());
    }

    #[test]
    fn ignores_function_passed_by_reference() {
        assert!(run("const double = (n: number) => n * 2;\nexport const doubled = list.map(double);").is_empty());
    }

    #[test]
    fn ignores_function_stored_in_an_object() {
        assert!(
            run("const double = (n: number) => n * 2;\nexport const handlers = { double };\nexport const t = double(1);")
                .is_empty()
        );
    }

    #[test]
    fn ignores_multi_statement_body() {
        assert!(
            run("function double(n: number) { const doubled = n * 2; return doubled; }\nexport const t = double(21);")
                .is_empty()
        );
    }

    #[test]
    fn ignores_body_over_the_token_budget() {
        let long = "function build(o: Order) { return { id: o.id, sku: o.sku, qty: o.qty, price: o.price, tax: o.tax, note: o.note }; }\nexport const row = build(order);";
        assert!(run(long).is_empty());
    }

    #[test]
    fn ignores_hook() {
        assert!(
            run("const useCartTotal = () => useStore((s) => s.total);\nexport function Row() { return useCartTotal(); }")
                .is_empty()
        );
    }

    #[test]
    fn ignores_react_component_returning_jsx() {
        assert!(
            run_tsx("const Badge = () => <span className=\"badge\" />;\nexport const Row = () => <td>{Badge()}</td>;")
                .is_empty()
        );
    }

    #[test]
    fn ignores_type_guard() {
        assert!(
            run("function isCat(a: Animal): a is Cat { return a.kind === 'cat'; }\nexport const t = isCat(a);")
                .is_empty()
        );
    }

    #[test]
    fn ignores_generator() {
        assert!(
            run("function* ids() { yield 1; }\nexport const all = [...ids()];").is_empty()
        );
    }

    #[test]
    fn ignores_overload_implementation() {
        assert!(
            run("function pick(a: string): string;\nfunction pick(a: number): number;\nfunction pick(a: any) { return a; }\nexport const t = pick(1);")
                .is_empty()
        );
    }

    #[test]
    fn ignores_recursive_helper() {
        assert!(run("function walk(n: Node) { return walk(n.next); }").is_empty());
    }

    #[test]
    fn ignores_shallow_passthrough_method_owned_elsewhere() {
        assert!(
            run("export class Api { private fetchOne(id: Id) { return this.request(id); } get(id: Id) { return this.fetchOne(id); } }")
                .is_empty()
        );
    }

    #[test]
    fn ignores_public_method() {
        assert!(
            run("export class Cart { subtotal() { return this.lines.length; } total() { return this.subtotal() + 1; } }")
                .is_empty()
        );
    }

    #[test]
    fn ignores_private_method_read_without_calling_it() {
        assert!(
            run("export class Cart { private subtotal() { return this.lines.length; } total() { return [this.subtotal].map((f) => f()); } }")
                .is_empty()
        );
    }

    #[test]
    fn ignores_private_getter() {
        assert!(
            run("export class Cart { private get subtotal() { return this.lines.length; } total() { return this.subtotal + 1; } }")
                .is_empty()
        );
    }

    #[test]
    fn ignores_object_method_shorthand() {
        assert!(
            run("const api = { unwrap(r: Res) { return r.data; } };\nexport const t = api.unwrap(x);").is_empty()
        );
    }

    #[test]
    fn ignores_constructed_function() {
        assert!(run("function Point(x: number) { return x; }\nexport const p = new Point(1);").is_empty());
    }

    #[test]
    fn token_count_measures_words_and_punctuation() {
        assert_eq!(token_count("r.data"), 3);
        assert_eq!(token_count("n * 2"), 3);
        assert_eq!(token_count("`a ${b} c`"), 1);
        assert_eq!(token_count("f(a, b)"), 6);
    }
}
