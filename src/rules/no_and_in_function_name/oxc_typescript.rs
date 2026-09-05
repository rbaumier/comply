//! no-and-in-function-name OXC backend — flag a function whose name glues two
//! parts together with `And` on a camelCase boundary, unless the function
//! returns the compound value those parts name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, type_annotation_is_type_predicate};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    ArrayExpressionElement, Expression, FunctionBody, ObjectPropertyKind, PropertyKey, Statement,
};
use std::sync::Arc;

pub struct Check;

/// Where the values a function returns live: the statements of a block body, or
/// the single expression of a concise arrow body.
enum ReturnSource<'a> {
    Block(&'a FunctionBody<'a>),
    Expression(&'a Expression<'a>),
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Function,
            AstType::MethodDefinition,
            AstType::VariableDeclarator,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (name, span_start, return_source) = match node.kind() {
            AstKind::Function(func) => {
                let Some(id) = &func.id else { return };
                // A `x is T` type-predicate return type marks a pure boolean type
                // guard (`isNullAndUnDef(v): v is null | undefined`). It is a query
                // with no side effect, so the CQS "two responsibilities" premise
                // cannot apply and the `And` merely joins conditions in one
                // compound predicate.
                if type_annotation_is_type_predicate(func.return_type.as_deref()) {
                    return;
                }
                (
                    id.name.as_str(),
                    id.span.start,
                    func.body.as_deref().map(ReturnSource::Block),
                )
            }
            AstKind::MethodDefinition(method) => {
                // An `override` method's name is dictated by the supertype it
                // overrides, not chosen by the author, so the "split into two
                // functions" remediation is impossible without breaking the
                // override contract (e.g. TypeORM `Repository.findAndCount`).
                if method.r#override {
                    return;
                }
                // A type-guard method (`isFooAndBar(v): v is Foo`) is a pure query;
                // see the `Function` arm above.
                if type_annotation_is_type_predicate(method.value.return_type.as_deref()) {
                    return;
                }
                let (name, span_start) = match &method.key {
                    PropertyKey::StaticIdentifier(id) => (id.name.as_str(), id.span.start),
                    // A `#private` method carries an author-chosen name just like
                    // a public one — `id.name` holds it without the `#` sigil.
                    PropertyKey::PrivateIdentifier(id) => (id.name.as_str(), id.span.start),
                    _ => return,
                };
                (
                    name,
                    span_start,
                    method.value.body.as_deref().map(ReturnSource::Block),
                )
            }
            AstKind::VariableDeclarator(decl) => {
                // Only flag when the value is an arrow or function expression.
                let (fn_return_type, return_source) = match decl.init.as_ref() {
                    Some(Expression::ArrowFunctionExpression(arrow)) => (
                        arrow.return_type.as_deref(),
                        // A concise body (`() => expr`) returns its expression;
                        // a block body returns through its `return` statements.
                        Some(match arrow.get_expression() {
                            Some(expression) => ReturnSource::Expression(expression),
                            None => ReturnSource::Block(&arrow.body),
                        }),
                    ),
                    Some(Expression::FunctionExpression(func)) => (
                        func.return_type.as_deref(),
                        func.body.as_deref().map(ReturnSource::Block),
                    ),
                    _ => return,
                };
                // A type-guard arrow/function (`const isFooAndBar = (v): v is Foo
                // => ...`) is a pure query; see the `Function` arm above.
                if type_annotation_is_type_predicate(fn_return_type) {
                    return;
                }
                let oxc_ast::ast::BindingPattern::BindingIdentifier(ref id) = decl.id else {
                    return;
                };
                (id.name.as_str(), id.span.start, return_source)
            }
            _ => return,
        };

        let segments = and_boundary_segments(name);
        if segments.is_empty() {
            return;
        }
        // The `And` may join the two nouns of one compound result rather than two
        // verbs; the returned members say which.
        if returns_the_named_members(return_source, &segments) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Function `{name}` has `And` in its name — that signals two \
                 responsibilities glued together (CQS violation). Split into two \
                 functions named after each responsibility and let the caller \
                 sequence them."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// The parts an identifier's `And` boundaries split it into —
/// `getCountryAndCallingCode` yields `["getCountry", "CallingCode"]`. A boundary
/// is an `And` preceded by a lowercase letter and followed by an uppercase one;
/// a name with no such boundary yields no segments.
fn and_boundary_segments(name: &str) -> Vec<&str> {
    let bytes = name.as_bytes();
    let mut segments = Vec::new();
    let mut start = 0;
    let mut i = 1;
    while i + 3 < bytes.len() {
        if bytes[i] == b'A'
            && bytes[i + 1] == b'n'
            && bytes[i + 2] == b'd'
            && bytes[i - 1].is_ascii_lowercase()
            && bytes[i + 3].is_ascii_uppercase()
        {
            segments.push(&name[start..i]);
            start = i + 3;
            i += 3;
        } else {
            i += 1;
        }
    }
    if !segments.is_empty() {
        segments.push(&name[start..]);
    }
    segments
}

/// True when every value the function returns is a compound literal whose members
/// are named after the `And`-joined segments of its name, in order — the "one
/// query, one compound result" shape (`getCountryAndCallingCode` returning
/// `[defaultCountry, defaultCallingCode]`), where the `And` joins the two nouns
/// of the result instead of two actions. Bail-out returns (`return;`,
/// `return undefined`) carry no result, so a guard clause cannot defeat the
/// match; a function that returns nothing corroborates nothing.
fn returns_the_named_members(return_source: Option<ReturnSource>, segments: &[&str]) -> bool {
    let mut returned = Vec::new();
    match return_source {
        None => return false,
        Some(ReturnSource::Expression(expression)) => returned.push(expression),
        Some(ReturnSource::Block(body)) => {
            collect_from_statements(&body.statements, &mut returned);
        }
    }

    let mut found_corroboration = false;
    for expression in returned {
        if is_undefined(expression) {
            continue;
        }
        if !members_are_named_after(expression, segments) {
            return false;
        }
        found_corroboration = true;
    }
    found_corroboration
}

/// Collects the argument of every `return` a statement can hide, without
/// descending into nested functions — they carry their own returns.
fn collect_returned_expressions<'ast, 'a>(
    statement: &'ast Statement<'a>,
    returned: &mut Vec<&'ast Expression<'a>>,
) {
    match statement {
        Statement::ReturnStatement(statement) => returned.extend(statement.argument.as_ref()),
        Statement::BlockStatement(block) => collect_from_statements(&block.body, returned),
        Statement::IfStatement(branch) => {
            collect_returned_expressions(&branch.consequent, returned);
            if let Some(alternate) = &branch.alternate {
                collect_returned_expressions(alternate, returned);
            }
        }
        Statement::ForStatement(loop_statement) => {
            collect_returned_expressions(&loop_statement.body, returned);
        }
        Statement::ForInStatement(loop_statement) => {
            collect_returned_expressions(&loop_statement.body, returned);
        }
        Statement::ForOfStatement(loop_statement) => {
            collect_returned_expressions(&loop_statement.body, returned);
        }
        Statement::WhileStatement(loop_statement) => {
            collect_returned_expressions(&loop_statement.body, returned);
        }
        Statement::DoWhileStatement(loop_statement) => {
            collect_returned_expressions(&loop_statement.body, returned);
        }
        Statement::LabeledStatement(labeled) => {
            collect_returned_expressions(&labeled.body, returned);
        }
        Statement::SwitchStatement(switch) => {
            for case in &switch.cases {
                collect_from_statements(&case.consequent, returned);
            }
        }
        Statement::TryStatement(try_statement) => {
            collect_from_statements(&try_statement.block.body, returned);
            if let Some(handler) = &try_statement.handler {
                collect_from_statements(&handler.body.body, returned);
            }
            if let Some(finalizer) = &try_statement.finalizer {
                collect_from_statements(&finalizer.body, returned);
            }
        }
        _ => {}
    }
}

fn collect_from_statements<'ast, 'a>(
    statements: &'ast [Statement<'a>],
    returned: &mut Vec<&'ast Expression<'a>>,
) {
    for statement in statements {
        collect_returned_expressions(statement, returned);
    }
}

/// True when a returned expression is a compound literal with one named member
/// per name segment, each named after its segment.
fn members_are_named_after(expression: &Expression, segments: &[&str]) -> bool {
    let Some(members) = compound_member_names(expression) else {
        return false;
    };
    members.len() == segments.len()
        && members
            .iter()
            .zip(segments)
            .all(|(member, segment)| shares_boundary_word(segment, member))
}

/// The member names of a compound literal — the identifiers of an array literal
/// or the identifier keys of an object literal. Any other element or property
/// shape (spread, hole, computed key, expression element) yields `None`: an
/// unnamed member names nothing.
fn compound_member_names<'ast, 'a>(expression: &'ast Expression<'a>) -> Option<Vec<&'ast str>> {
    match expression.get_inner_expression() {
        Expression::ArrayExpression(array) => array
            .elements
            .iter()
            .map(|element| match element {
                ArrayExpressionElement::Identifier(id) => Some(id.name.as_str()),
                _ => None,
            })
            .collect(),
        Expression::ObjectExpression(object) => object
            .properties
            .iter()
            .map(|property| match property {
                ObjectPropertyKind::ObjectProperty(property) => match &property.key {
                    PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
                    _ => None,
                },
                ObjectPropertyKind::SpreadProperty(_) => None,
            })
            .collect(),
        _ => None,
    }
}

fn is_undefined(expression: &Expression) -> bool {
    matches!(
        expression.get_inner_expression(),
        Expression::Identifier(id) if id.name == "undefined"
    )
}

/// True when a name segment and a returned member name share a leading or a
/// trailing word, case-insensitively. That identity ties `getCountry` to
/// `defaultCountry` and `CallingCodeFromOneOfThem` to `callingCode`: the segment
/// names the member, modulo the verb it leads with or a qualifier phrase on
/// either side.
fn shares_boundary_word(segment: &str, member: &str) -> bool {
    let (segment_first, segment_last) = boundary_words(segment);
    let (member_first, member_last) = boundary_words(member);
    [segment_first, segment_last].iter().any(|word| {
        word.eq_ignore_ascii_case(member_first) || word.eq_ignore_ascii_case(member_last)
    })
}

/// The leading and trailing words of an identifier, split on camelCase and `_`
/// boundaries — `defaultCallingCode` yields `("default", "Code")` and `country`
/// yields `("country", "country")`.
fn boundary_words(name: &str) -> (&str, &str) {
    let name = name.trim_matches('_');
    let bytes = name.as_bytes();
    let mut first_end = bytes.len();
    let mut last_start = 0;
    for i in 1..bytes.len() {
        if starts_word(bytes, i) {
            first_end = first_end.min(i);
            last_start = i;
        }
    }
    (&name[..first_end], &name[last_start..])
}

/// True when byte `i` starts a word: the first letter after a `_`, an uppercase
/// letter following a lowercase one or a digit, or the uppercase letter that
/// ends an acronym run and starts a lowercase word (`URLParser` → `Parser`).
fn starts_word(bytes: &[u8], i: usize) -> bool {
    if bytes[i] == b'_' {
        return false;
    }
    if bytes[i - 1] == b'_' {
        return true;
    }
    if !bytes[i].is_ascii_uppercase() {
        return false;
    }
    bytes[i - 1].is_ascii_lowercase()
        || bytes[i - 1].is_ascii_digit()
        || (bytes[i - 1].is_ascii_uppercase()
            && bytes.get(i + 1).is_some_and(u8::is_ascii_lowercase))
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_override_method_implementing_inherited_contract() {
        // Regression for rbaumier/comply#7423 — twentyhq/twenty
        // `WorkspaceRepository.findAndCount`. The `override` keyword binds the
        // method's name to the supertype's contract (TypeORM `Repository`), so
        // it cannot be renamed or split without breaking the override.
        let src = r#"class WorkspaceRepository<T> extends Repository<T> {
            override async findAndCount(o?: FindManyOptions<T>): Promise<[T[], number]> {
                return [[], 0];
            }
        }"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_non_override_method_with_and_boundary() {
        // Negative space for #7423: a non-`override` method whose name has an
        // `And` boundary has no supertype dictating its name — it stays flagged.
        let src = "class C { invalidateAndRecompute() {} }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_second_non_override_method_with_and_boundary() {
        // Second control for #7423: another non-`override` `And`-boundary method.
        let src = "class C { getTargetEntityAndOperationType() {} }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_private_method_with_and_boundary() {
        // Regression for rbaumier/comply#8153 — a `#private` method's name is
        // authored exactly like a public one, so renaming `fetchAndParse` to
        // `#fetchAndParse` must not silence the rule.
        let src = "class Private {\n\
                   #fetchAndParse(url: string): string { return url; }\n\
                   use(): string { return this.#fetchAndParse('u'); }\n\
                   }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_free_function_with_and_boundary() {
        // The `override` exemption is scoped to method definitions; a free
        // function with an `And` boundary stays flagged.
        assert_eq!(run_on("function getFooAndBar() {}").len(), 1);
    }

    #[test]
    fn still_flags_arrow_assigned_const_with_and_boundary() {
        // The `override` exemption is scoped to method definitions; an
        // arrow-assigned const (the VariableDeclarator arm) stays flagged.
        assert_eq!(run_on("const doFooAndBar = () => {};").len(), 1);
    }

    #[test]
    fn allows_function_type_guard_predicate() {
        // Regression for rbaumier/comply#7508 — jekip/naive-ui-admin
        // `isNullAndUnDef`. A `val is T` return type marks a pure boolean type
        // guard; CQS (which separates commands from queries) cannot apply, so
        // the `And` joining two conditions in one compound predicate is fine.
        let src = "export function isNullAndUnDef(val: unknown): val is null | undefined {\n\
                   return isUnDef(val) && isNull(val);\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_arrow_type_guard_predicate() {
        // #7508: the exemption reaches the VariableDeclarator arm — an arrow
        // whose return type is a type predicate is a pure query.
        assert!(run_on("const isFooAndBar = (v: unknown): v is Foo => true;").is_empty());
    }

    #[test]
    fn allows_method_type_guard_predicate() {
        // #7508: the exemption reaches the MethodDefinition arm — a method whose
        // return type is a type predicate is a pure query.
        let src = "class C { isFooAndBar(v: unknown): v is Foo { return true; } }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_command_function_with_non_predicate_return() {
        // Negative space for #7508: a `void`-returning command with an `And`
        // boundary is exactly the CQS violation the rule targets — still flagged.
        assert_eq!(run_on("function saveAndNotify(): void {}").len(), 1);
    }

    #[test]
    fn still_flags_function_with_and_boundary_and_no_return_annotation() {
        // Negative space for #7508: no return-type annotation means no type
        // predicate, so the exemption does not apply.
        assert_eq!(run_on("function loadAndParse() {}").len(), 1);
    }

    #[test]
    fn still_flags_non_predicate_arrow_with_and_boundary() {
        // Negative space for #7508: an arrow with a non-predicate return type
        // stays flagged.
        assert_eq!(run_on("const doFooAndBar = (): void => {};").len(), 1);
    }

    #[test]
    fn allows_pair_query_returning_its_two_named_members() {
        // Regression for rbaumier/comply#8104 — libphonenumber-js
        // `AsYouType.getCountryAndCallingCode`. The `And` joins the two nouns of
        // one compound result, not two verbs: the body returns the very pair the
        // name announces, so there are no two responsibilities to split.
        let src = "class AsYouType {\n\
                   getCountryAndCallingCode(options) {\n\
                     let defaultCountry;\n\
                     let defaultCallingCode;\n\
                     if (options) { defaultCountry = options.defaultCountry; }\n\
                     return [defaultCountry, defaultCallingCode];\n\
                   }\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_pair_query_returning_an_object_literal() {
        // #8104: the object-literal form of the same shape — the keys are the
        // `And`-joined segments (libphonenumber-js
        // `getCountryAndCallingCodeFromOneOfThem`, whose trailing qualifier
        // phrase rides on the second segment).
        let src = "function getCountryAndCallingCodeFromOneOfThem(input) {\n\
                   let country;\n\
                   let callingCode;\n\
                   return { country, callingCode };\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_pair_query_written_as_a_concise_arrow() {
        // #8104: the exemption reaches the VariableDeclarator arm — a concise
        // arrow body is the returned value.
        let src = "const getIdAndName = (row) => ({ id: row.id, name: row.name });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_pair_query_behind_a_bail_out_guard() {
        // #8104: a guard clause returning nothing is a bail-out, not a result, so
        // it must not defeat the corroboration.
        let src = "function getCountryAndCallingCode(o) {\n\
                   if (!o) return;\n\
                   return [country, callingCode];\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_two_verbs_returning_a_single_value() {
        // Negative space for #8104 — libphonenumber-js `parseAndVerify`. Two
        // verbs, one returned match object: nothing corroborates a compound
        // result, so the CQS violation stays flagged.
        assert_eq!(run_on("function parseAndVerify(text) { return match(text); }").len(), 1);
    }

    #[test]
    fn still_flags_pair_name_returning_a_single_member() {
        // Negative space for #8104: the name announces a pair but the body returns
        // one member, so the compound-result claim is not corroborated.
        assert_eq!(
            run_on("function getCountryAndCallingCode(o) { return country; }").len(),
            1
        );
    }

    #[test]
    fn still_flags_pair_name_returning_unrelated_members() {
        // Negative space for #8104: a compound return whose members are named
        // after neither segment corroborates nothing.
        assert_eq!(
            run_on("function loadAndParse(input) { return [handle, buffer]; }").len(),
            1
        );
    }
}
