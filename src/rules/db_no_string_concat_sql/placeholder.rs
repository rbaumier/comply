//! Proof that an interpolated expression yields *only* SQL placeholders.
//!
//! The canonical dynamic IN-clause idiom builds a bind-parameter list by
//! repeating a constant placeholder, never the data itself:
//!
//! ```js
//! const placeholders = ids.map(() => '?').join(',');  // "?,?,?"
//! db.all(`SELECT * FROM t WHERE id IN (${placeholders})`, ids);
//! ```
//!
//! The interpolation expands to `?,?,?` — the actual values are bound
//! separately as query parameters — so it carries no data and is not an
//! injection. This module recognises that idiom, and only that idiom: a
//! `<recv>.map(<cb>).join(<sep>)` / `Array(n).fill(<ph>).join(<sep>)` /
//! `Array.from(<src>, <cb>).join(<sep>)` whose produced element is provably a
//! constant placeholder and whose separator is a constant placeholder string.
//!
//! Security contract: an interpolation is exempted *only* when placeholder-only
//! production is proven. Anything that could embed the element value — a
//! callback returning or deriving from the element parameter, a non-literal
//! separator, an unresolvable reference — is rejected, so value interpolation
//! still fires.

use oxc_ast::ast::{Argument, Expression, FormalParameters, Statement};

/// Whether interpolating `expr` into a SQL template literal is provably safe
/// because `expr` evaluates to a string of SQL placeholders only (`?`, `$1`),
/// never data.
///
/// Resolves a bare identifier to its `const`/`let` initializer once, then
/// checks the placeholder-join idiom on the resolved expression.
pub(super) fn interpolation_is_provably_placeholder_only<'a>(
    expr: &Expression<'a>,
    semantic: &oxc_semantic::Semantic<'a>,
) -> bool {
    let resolved = resolve_to_initializer(expr, semantic).unwrap_or(expr);
    expr_is_placeholder_join(resolved)
}

/// If `expr` is a reference to a `const`/`let` binding, the binding's
/// initializer expression; otherwise `None`. Used to follow one level of
/// `const placeholders = …` indirection. A function parameter, imported binding,
/// `var`, or a binding without an initializer resolves to `None`.
fn resolve_to_initializer<'a>(
    expr: &Expression<'a>,
    semantic: &oxc_semantic::Semantic<'a>,
) -> Option<&'a Expression<'a>> {
    use oxc_ast::AstKind;
    use oxc_ast::ast::VariableDeclarationKind;

    let Expression::Identifier(ident) = expr.without_parentheses() else {
        return None;
    };
    let ref_id = ident.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            if !matches!(
                decl.kind,
                VariableDeclarationKind::Const | VariableDeclarationKind::Let
            ) {
                return None;
            }
            return decl.init.as_ref();
        }
    }
    None
}

/// Whether `expr` is a placeholder-join: a `.join(<sep>)` over an array that
/// produces only constant placeholder elements, with a constant placeholder
/// separator (`','`, `', '`).
fn expr_is_placeholder_join(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr.without_parentheses() else {
        return false;
    };
    let Expression::StaticMemberExpression(member) = call.callee.without_parentheses() else {
        return false;
    };
    if member.property.name != "join" {
        return false;
    }
    // join's separator must be a constant placeholder string (or absent → ",").
    match call.arguments.first() {
        None => {}
        Some(Argument::SpreadElement(_)) => return false,
        Some(arg) => {
            let Some(sep) = arg.as_expression() else {
                return false;
            };
            if !string_is_placeholder_only(sep) {
                return false;
            }
        }
    }
    array_yields_only_placeholders(&member.object)
}

/// Whether the array receiver of `.join()` produces only constant placeholder
/// elements: `<recv>.map(<cb>)`, `<recv>.fill(<ph>)`, or
/// `Array.from(<src>, <cb>)`.
fn array_yields_only_placeholders(array: &Expression) -> bool {
    let Expression::CallExpression(call) = array.without_parentheses() else {
        return false;
    };
    // `Array.from(src, cb)` — second arg is the mapping callback.
    if let Expression::StaticMemberExpression(member) = call.callee.without_parentheses()
        && member.property.name == "from"
        && let Expression::Identifier(obj) = member.object.without_parentheses()
        && obj.name == "Array"
    {
        let Some(cb) = call.arguments.get(1).and_then(Argument::as_expression) else {
            return false;
        };
        return callback_produces_placeholder(cb);
    }
    let Expression::StaticMemberExpression(member) = call.callee.without_parentheses() else {
        return false;
    };
    match member.property.name.as_str() {
        // `<recv>.map(cb)` — element produced by the callback.
        "map" => call
            .arguments
            .first()
            .and_then(Argument::as_expression)
            .is_some_and(callback_produces_placeholder),
        // `<recv>.fill('?')` — element is the (constant) fill value.
        "fill" => call
            .arguments
            .first()
            .and_then(Argument::as_expression)
            .is_some_and(string_is_placeholder_only),
        _ => false,
    }
}

/// Whether a `.map` / `Array.from` callback provably returns a constant
/// placeholder. The callback's returned expression must be placeholder-shaped
/// AND must not reference the element-*value* parameter (the first parameter);
/// only the index (second) parameter may be used, so no data can be embedded.
fn callback_produces_placeholder(cb: &Expression) -> bool {
    let (params, body_expr) = match cb.without_parentheses() {
        Expression::ArrowFunctionExpression(arrow) => {
            let body_expr = arrow_body_return(arrow);
            (&arrow.params, body_expr)
        }
        Expression::FunctionExpression(func) => {
            let body_expr = func
                .body
                .as_ref()
                .and_then(|b| single_return_expression(&b.statements));
            (&func.params, body_expr)
        }
        _ => return false,
    };
    let Some(body_expr) = body_expr else {
        return false;
    };
    // The only identifier the body may reference is the index parameter (a
    // number → positional placeholders are constant). The element-value
    // parameter, and any closure variable, could carry data, so referencing
    // anything else is not provably placeholder-only.
    let index_param = index_param_name(params);
    expression_is_placeholder_shaped(body_expr, index_param.as_deref())
}

/// The returned expression of an arrow function: its concise body, or the sole
/// `return <expr>` of a block body. `None` for a block with no/multiple/void
/// returns (not provable).
fn arrow_body_return<'a>(
    arrow: &'a oxc_ast::ast::ArrowFunctionExpression<'a>,
) -> Option<&'a Expression<'a>> {
    if arrow.expression {
        // Concise body: the single statement is an `ExpressionStatement`.
        return arrow.body.statements.first().and_then(|stmt| match stmt {
            Statement::ExpressionStatement(es) => Some(&es.expression),
            _ => None,
        });
    }
    single_return_expression(&arrow.body.statements)
}

/// The expression of a function body that consists of exactly one
/// `return <expr>;`. `None` otherwise.
fn single_return_expression<'a>(
    statements: &'a oxc_allocator::Vec<'a, Statement<'a>>,
) -> Option<&'a Expression<'a>> {
    let [Statement::ReturnStatement(ret)] = statements.as_slice() else {
        return None;
    };
    ret.argument.as_ref()
}

/// The name bound to the second (index) parameter of a callback, when it is a
/// plain identifier pattern (`(_, i) => …`). `None` when there is no second
/// parameter (`() => '?'`, `(x) => '?'`) or it is a destructuring pattern. The
/// index is the *only* identifier a placeholder-shaped body may reference.
fn index_param_name(params: &FormalParameters) -> Option<String> {
    use oxc_ast::ast::BindingPattern;
    let second = params.items.get(1)?;
    match &second.pattern {
        BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
        _ => None,
    }
}

/// Whether `expr` is provably a constant placeholder. The only identifier
/// permitted is `index_param` (the callback's index parameter, a number, so
/// positional placeholders like `$${i + 1}` stay constant); every other
/// identifier — the element-value parameter, a closure variable — could carry
/// data and is rejected.
///
/// Placeholder-shaped expressions: a placeholder-only string literal, a number
/// literal, the index parameter, a `+`/template composition of those, or a
/// parenthesised such expression.
fn expression_is_placeholder_shaped(expr: &Expression, index_param: Option<&str>) -> bool {
    match expr.without_parentheses() {
        Expression::StringLiteral(lit) => is_placeholder_chars(lit.value.as_str()),
        Expression::NumericLiteral(_) => true,
        Expression::Identifier(id) => index_param == Some(id.name.as_str()),
        Expression::BinaryExpression(bin) => {
            bin.operator == oxc_ast::ast::BinaryOperator::Addition
                && expression_is_placeholder_shaped(&bin.left, index_param)
                && expression_is_placeholder_shaped(&bin.right, index_param)
        }
        Expression::TemplateLiteral(tpl) => {
            // An empty quasi (the text between/around `${…}`) contributes no
            // characters, so it is vacuously fine; a non-empty one must be
            // placeholder-only.
            tpl.quasis
                .iter()
                .filter(|q| !q.value.raw.is_empty())
                .all(|q| is_placeholder_chars(q.value.raw.as_str()))
                && tpl
                    .expressions
                    .iter()
                    .all(|e| expression_is_placeholder_shaped(e, index_param))
        }
        _ => false,
    }
}

/// Whether `expr` is a string literal consisting solely of SQL-placeholder
/// characters.
fn string_is_placeholder_only(expr: &Expression) -> bool {
    matches!(
        expr.without_parentheses(),
        Expression::StringLiteral(lit) if is_placeholder_chars(lit.value.as_str())
    )
}

/// Whether every character of `s` belongs to the SQL-placeholder alphabet, so
/// the literal can only ever be a placeholder, never data.
///
/// Always allowed: `?` (anonymous), `$`/`:` (positional/named sigils), the
/// `,`/`(`/`)`/whitespace that surround a placeholder list, and digits. ASCII
/// letters (a named-placeholder body such as `:p1`/`$arg`) are allowed *only*
/// when the literal opens with a sigil — a bare word like `name` has no sigil
/// and is rejected as possible data. Quotes are never allowed. An empty string
/// is not a placeholder.
fn is_placeholder_chars(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let opens_with_sigil = s.trim_start().starts_with(['$', ':']);
    s.chars().all(|c| {
        matches!(c, '?' | '$' | ':' | ',' | '(' | ')')
            || c.is_ascii_digit()
            || c.is_ascii_whitespace()
            || (opens_with_sigil && (c.is_ascii_alphabetic() || c == '_'))
    })
}
