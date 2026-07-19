//! prefer-string-replace-all OXC backend — flag `.replace(/pattern/g, ...)`.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, FunctionBody, ObjectExpression, RegExpFlags, Statement};

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".replace"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "replace" {
            return;
        }

        // First argument must be a regex literal with the `g` flag.
        let Some(first_arg) = call.arguments.first() else { return };
        let Argument::RegExpLiteral(regex) = first_arg else { return };

        if !regex.regex.flags.contains(RegExpFlags::G) {
            return;
        }

        // `String#replaceAll(string)` can only replace a fixed literal substring.
        // A regex with anchors, alternation, quantifiers, classes, or assertions
        // is not equivalent to any constant string, so suggesting `.replaceAll`
        // would silently change behavior. Only flag fixed-literal patterns.
        if !regex_pattern_is_fixed_literal(regex.regex.pattern.text.as_str()) {
            return;
        }

        // The chain root decides whether `.replace` is `String#replace` or a
        // custom builder's own method. When the chain bottoms out at a call to a
        // local function returning an object literal that defines its own
        // `replace`, every link targets that method (which has no `replaceAll`),
        // so skip the whole chain. See `chain_root_is_object_builder`.
        if chain_root_is_object_builder(&member.object, semantic) {
            return;
        }

        // Anchor at the `replace` property identifier. For a chained member call
        // (`s.replace(/a/g).replace(/b/g)`), oxc spans every `CallExpression` from
        // the chain root, so `call.span.start` would stack all diagnostics on the
        // leftmost object; `member.property.span.start` points at each `.replace`.
        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `String#replaceAll()` over `String#replace()` with a global regex."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Whether a regex source pattern matches exactly one constant substring, so
/// `String#replace(/p/g, r)` can be rewritten as `String#replaceAll("p", r)`
/// without changing behavior.
///
/// Walks the source char-by-char. An unescaped regex metacharacter (anchor,
/// alternation, quantifier, group, or class delimiter) means the pattern is not
/// a fixed string. A backslash escapes the next char: escaping an ASCII
/// punctuation metacharacter (`\.`, `\+`, `\\`, `\/`, ...) yields that literal
/// punctuation char, but a backslash before a letter or digit introduces a class
/// shorthand (`\d`, `\w`, `\b`), an assertion, or a numeric/unicode escape
/// (`\0`, `\n`, `\xNN`, `\uNNNN`), none of which denote a fixed substring. A
/// dangling trailing backslash is treated as non-literal. When in doubt, return
/// false so the rule stays silent rather than risk a behavior-changing rewrite.
fn regex_pattern_is_fixed_literal(pattern: &str) -> bool {
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                // Escaped ASCII punctuation denotes the literal punctuation char.
                Some(next) if next.is_ascii_punctuation() => {}
                // Class shorthand, assertion, numeric/unicode escape, or a
                // dangling backslash — not a fixed substring.
                _ => return false,
            }
        } else if matches!(
            c,
            '^' | '$' | '.' | '[' | ']' | '(' | ')' | '|' | '+' | '*' | '?' | '{' | '}'
        ) {
            return false;
        }
    }
    true
}

/// Peel `.replace(...)` call links off `receiver` to reach the chain root — the
/// left-most object that is not itself a `.replace(...)` call. For
/// `a.replace(x).replace(y)`, every link's receiver bottoms out at `a`, so the
/// root, not the immediate receiver, decides the chain's nature (`String#replace`
/// returns a string, so a real String chain stays a String chain throughout).
fn chain_root<'a, 'r>(receiver: &'r Expression<'a>) -> &'r Expression<'a> {
    let mut current = receiver;
    loop {
        let Expression::CallExpression(call) = current else {
            return current;
        };
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return current;
        };
        if member.property.name.as_str() != "replace" {
            return current;
        }
        current = &member.object;
    }
}

/// True when the `.replace` chain with receiver `receiver` is rooted in a call to
/// a local function returning an object literal that defines its own `replace`
/// property. There `.replace` resolves to the builder's method, not
/// `String#replace`, and `replaceAll` would not exist on it.
fn chain_root_is_object_builder<'a>(
    receiver: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Expression::CallExpression(root_call) = chain_root(receiver) else {
        return false;
    };
    let Expression::Identifier(callee) = &root_call.callee else {
        return false;
    };
    resolve_callee_return_expr(callee, semantic)
        .is_some_and(|ret| return_expr_defines_replace(ret, semantic))
}

/// Resolve a callee identifier to the return-value expression of the local
/// function it binds to — a function declaration, or a `const`/`let` bound to a
/// function/arrow expression. Returns the first top-level `return` argument (or
/// the expression body of a concise arrow). `None` when the identifier is
/// unresolved, imported, or bound to anything else.
fn resolve_callee_return_expr<'a>(
    callee: &oxc_ast::ast::IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a Expression<'a>> {
    let ref_id = callee.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        match kind {
            AstKind::Function(func) => return func.body.as_ref().and_then(|b| body_return_expr(b)),
            AstKind::VariableDeclarator(decl) => {
                return match decl.init.as_ref()? {
                    Expression::ArrowFunctionExpression(arrow) => arrow_return_expr(arrow),
                    Expression::FunctionExpression(func) => {
                        func.body.as_ref().and_then(|b| body_return_expr(b))
                    }
                    _ => None,
                };
            }
            _ => {}
        }
    }
    None
}

/// First top-level `return` argument in a function body. Does not descend into
/// nested control flow or functions — the object-builder pattern returns the
/// builder at the top level.
fn body_return_expr<'a>(body: &'a FunctionBody<'a>) -> Option<&'a Expression<'a>> {
    body.statements.iter().find_map(|stmt| match stmt {
        Statement::ReturnStatement(ret) => ret.argument.as_ref(),
        _ => None,
    })
}

/// The expression a concise arrow returns (`() => expr`), or the first top-level
/// `return` argument of a block-bodied arrow.
fn arrow_return_expr<'a>(
    arrow: &'a oxc_ast::ast::ArrowFunctionExpression<'a>,
) -> Option<&'a Expression<'a>> {
    if arrow.expression {
        return match arrow.body.statements.first() {
            Some(Statement::ExpressionStatement(stmt)) => Some(&stmt.expression),
            _ => None,
        };
    }
    body_return_expr(&arrow.body)
}

/// True when `expr` is — or resolves through one `const`/`let` binding to — an
/// object literal that defines a `replace` property.
fn return_expr_defines_replace<'a>(
    expr: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    match expr {
        Expression::ObjectExpression(obj) => object_defines_replace(obj),
        Expression::Identifier(id) => {
            resolve_var_object(id, semantic).is_some_and(object_defines_replace)
        }
        _ => false,
    }
}

/// Resolve an identifier to a `const`/`let` binding whose initializer is an
/// object literal, returning that literal.
fn resolve_var_object<'a>(
    id: &oxc_ast::ast::IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<&'a ObjectExpression<'a>> {
    let ref_id = id.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::VariableDeclarator(decl) = kind {
            return match decl.init.as_ref()? {
                Expression::ObjectExpression(obj) => Some(obj.as_ref()),
                _ => None,
            };
        }
    }
    None
}

/// True when an object literal has a property named `replace` — a value property
/// or a method.
fn object_defines_replace(obj: &ObjectExpression) -> bool {
    use oxc_ast::ast::{ObjectPropertyKind, PropertyKey};

    obj.properties.iter().any(|prop| {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            return false;
        };
        match &p.key {
            PropertyKey::StaticIdentifier(key) => key.name == "replace",
            PropertyKey::StringLiteral(key) => key.value == "replace",
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_by_id("prefer-string-replace-all", source, "t.ts")
    }

    #[test]
    fn flags_replace_with_global_regex() {
        let d = run(r#"str.replace(/foo/g, 'bar')"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-string-replace-all");
        // Anchored at `replace` (column 5), not the `str` chain root (column 1).
        assert_eq!((d[0].line, d[0].column), (1, 5));
    }

    #[test]
    fn allows_replace_without_global() {
        assert!(run(r#"str.replace(/foo/, 'bar')"#).is_empty());
    }

    #[test]
    fn allows_replace_with_string_arg() {
        assert!(run(r#"str.replace('foo', 'bar')"#).is_empty());
    }

    #[test]
    fn allows_replace_all_already() {
        assert!(run(r#"str.replaceAll('foo', 'bar')"#).is_empty());
    }

    // Regression for #3818: a chained `.replace().replace()` must emit one
    // diagnostic per `.replace`, each anchored at its own `replace` method, not
    // all stacked on the chain-root identifier. oxc spans every CallExpression
    // in the chain from the leftmost object, so anchoring at `call.span.start`
    // collapsed every link onto the same column.
    #[test]
    fn chained_replace_anchors_each_link_at_its_own_method() {
        let source = "export function f(s: string) {\n  return s.replace(/#/g, \"%23\").replace(/\\?/g, \"%3F\");\n}";
        let d = run(source);
        assert_eq!(d.len(), 2, "one diagnostic per global-regex .replace");

        // Both links are on line 2; the chain root `s` is at column 10.
        assert_eq!(d[0].line, 2);
        assert_eq!(d[1].line, 2);

        // The two `replace` methods sit at distinct columns: `  return s.` is
        // 11 chars so the first `replace` starts at column 12; the second follows
        // `.replace(/#/g, "%23").` and starts at column 33. (Emission order follows
        // AST traversal — outer call first — so compare as a sorted set.)
        let mut columns: Vec<usize> = d.iter().map(|diag| diag.column).collect();
        columns.sort_unstable();
        assert_eq!(columns, vec![12, 33]);

        // Neither diagnostic is anchored at the chain-root token `s` (column 10).
        assert!(columns.iter().all(|&c| c != 10));
    }

    // Regression for #6662: a global regex whose pattern is not equivalent to a
    // fixed literal must not be flagged — `.replaceAll(string)` would silently
    // change behavior.
    #[test]
    fn allows_anchors_and_alternation() {
        // `/^"|"$/g` — anchors (`^`, `$`) plus alternation (`|`).
        assert!(run(r#"str.replace(/^"|"$/g, "")"#).is_empty());
    }

    #[test]
    fn allows_quantifier() {
        // `/\\+/g` — one-or-more backslashes (`+` quantifier), not a fixed string.
        assert!(run(r"str.replace(/\\+/g, 'x')").is_empty());
    }

    #[test]
    fn allows_character_class() {
        assert!(run(r"str.replace(/[ab]/g, 'x')").is_empty());
    }

    #[test]
    fn allows_class_shorthand() {
        assert!(run(r"str.replace(/\d/g, 'x')").is_empty());
    }

    #[test]
    fn flags_escaped_punctuation_literal() {
        // `/\./g` matches a literal dot — a fixed substring, still convertible.
        let d = run(r"str.replace(/\./g, 'x')");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_escaped_quote_literal() {
        // `/\\"/g` matches the fixed two-char string `\"` — still convertible.
        let d = run(r#"str.replace(/\\"/g, '"')"#);
        assert_eq!(d.len(), 1);
    }

    // Regression for #6248: `.replace(/g/, …)` on the object returned by a local
    // `edit()` builder targets the builder's own `.replace` method, not
    // `String#replace`. Both the `/bull/g` and `/blockCode/g` links must be
    // exempt, proving the chain-root walk reaches the `edit(...)` call.
    #[test]
    fn allows_replace_chain_rooted_in_local_object_builder() {
        let source = r#"
function edit(regex, opt = '') {
  let source = typeof regex === 'string' ? regex : regex.source;
  const obj = {
    replace: (name, val) => { source = source.replace(name, val); return obj; },
    getRegex: () => new RegExp(source, opt),
  };
  return obj;
}
const lheading = edit(lheadingCore).replace(/bull/g, bullet).replace(/blockCode/g, x).getRegex();
"#;
        assert!(run(source).is_empty(), "builder .replace chain must not be flagged");
    }

    // Regression for #6248: the exemption requires the returned literal to define
    // its own `replace`. A local function whose object lacks `replace` is a
    // genuine `String#replace` target and stays flagged.
    #[test]
    fn flags_replace_when_local_object_lacks_replace_member() {
        let source = "function mk() { return { getRegex() {} }; }\nmk().replace(/a/g, 'b');";
        let d = run(source);
        assert_eq!(d.len(), 1);
    }

    // Regression for #6248: a cross-file (imported) callee cannot be resolved to
    // a local function, so the object-builder exemption must NOT fire — the
    // chain stays flagged.
    #[test]
    fn flags_replace_chain_rooted_in_imported_function() {
        let source = "import { edit } from './x';\nedit('a').replace(/a/g, 'b');";
        let d = run(source);
        assert_eq!(d.len(), 1);
    }
}
