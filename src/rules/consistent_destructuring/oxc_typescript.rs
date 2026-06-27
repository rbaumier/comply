//! consistent-destructuring OXC backend.
//!
//! Flags member expressions like `user.age` when the same object was
//! already destructured earlier in the same scope. When both the
//! destructuring source and the member-access object resolve to a binding,
//! matching is keyed on that binding's symbol, so two identically named
//! variables in unrelated scopes never cross-trigger.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_semantic::SymbolId;
use oxc_span::GetSpan;
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let source = ctx.source;
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        // Phase 1: collect all destructuring declarations
        struct Destructured {
            obj_text: String,
            obj_symbol: Option<SymbolId>,
            end_byte: u32,
            fn_range: Option<(u32, u32)>,
            block_range: Option<(u32, u32)>,
        }
        let mut destructured: Vec<Destructured> = Vec::new();

        // Phase 2: collect all member expression candidates
        struct Candidate {
            obj_text: String,
            obj_symbol: Option<SymbolId>,
            prop_text: String,
            start_byte: u32,
        }
        let mut candidates: Vec<Candidate> = Vec::new();

        // Properties that are written somewhere on an object (`obj.prop = ...`,
        // `obj.prop += ...`, `obj.prop++`, etc.). A written property cannot be
        // safely cached via destructuring — the destructured copy would be the
        // pre-mutation value — so reads of such a property are exempt.
        // Keyed by the object's binding symbol AND source text, so a write on
        // a different object never exempts a read on the destructured one —
        // even when neither base resolves to a binding (unresolved globals
        // both carry a `None` symbol and are disambiguated by text).
        let mut mutated_props: FxHashSet<(Option<SymbolId>, String, String)> = FxHashSet::default();

        for node in nodes.iter() {
            match node.kind() {
                AstKind::VariableDeclarator(decl) => {
                    let BindingPattern::ObjectPattern(pattern) = &decl.id else {
                        continue;
                    };
                    let Some(init) = &decl.init else { continue };
                    if !is_simple_expression(init) {
                        continue;
                    }
                    let obj_text = &source[init.span().start as usize..init.span().end as usize];

                    let has_rest = pattern.rest.is_some();
                    if has_rest {
                        continue;
                    }

                    let mut has_props = false;
                    for prop in &pattern.properties {
                        if let PropertyKey::StaticIdentifier(_) = &prop.key { has_props = true; }
                    }
                    if !has_props {
                        continue;
                    }

                    // `var` hoists to the enclosing function, so its binding
                    // stays live outside the block it is written in; only
                    // `let`/`const`/`using` are block-scoped.
                    let is_var = matches!(
                        nodes.get_node(nodes.parent_id(node.id())).kind(),
                        AstKind::VariableDeclaration(vd) if vd.kind.is_var()
                    );

                    // Walk up once: record the innermost enclosing
                    // `BlockStatement` (a block-scoped binding's lexical extent)
                    // and stop at the enclosing function. `block_range` stays
                    // `None` when the declarator sits directly in a
                    // function/program body, or when it is a hoisted `var`, so
                    // the function-level check governs those cases.
                    let (fn_range, block_range) = {
                        let mut fn_range = None;
                        let mut block_range = None;
                        for ancestor in nodes.ancestors(node.id()) {
                            match ancestor.kind() {
                                AstKind::BlockStatement(b) => {
                                    if block_range.is_none() && !is_var {
                                        block_range = Some((b.span.start, b.span.end));
                                    }
                                }
                                AstKind::Function(f) => {
                                    fn_range = Some((f.span.start, f.span.end));
                                    break;
                                }
                                AstKind::ArrowFunctionExpression(a) => {
                                    fn_range = Some((a.span.start, a.span.end));
                                    break;
                                }
                                _ => {}
                            }
                        }
                        (fn_range, block_range)
                    };
                    let obj_symbol = identifier_symbol(init, semantic);
                    destructured.push(Destructured {
                        obj_text: obj_text.to_string(),
                        obj_symbol,
                        end_byte: decl.span.end,
                        fn_range,
                        block_range,
                    });
                }
                AstKind::StaticMemberExpression(member) => {
                    // Skip if parent is a member expression (nested: user.address.city)
                    let parent_id = nodes.parent_id(node.id());
                    if parent_id != node.id() {
                        let parent = nodes.get_node(parent_id);
                        match parent.kind() {
                            AstKind::StaticMemberExpression(parent_mem) => {
                                // If we are the object of the parent, skip (nested access)
                                if parent_mem.object.span() == member.span() {
                                    continue;
                                }
                            }
                            AstKind::CallExpression(call) => {
                                // Skip if this is the callee of a call (user.greet())
                                if call.callee.span() == member.span() {
                                    continue;
                                }
                            }
                            AstKind::AssignmentExpression(assign) => {
                                // Skip assignments (user.age = 5) and record
                                // the mutation so later reads stay exempt.
                                if assign.left.span().start == member.span().start
                                    && assign.left.span().end == member.span().end
                                {
                                    mutated_props.insert(write_key(member, source, semantic));
                                    continue;
                                }
                            }
                            AstKind::UpdateExpression(update) => {
                                // Skip increment/decrement (user.age++, --user.age)
                                // and record the mutation.
                                if update.argument.span().start == member.span().start
                                    && update.argument.span().end == member.span().end
                                {
                                    mutated_props.insert(write_key(member, source, semantic));
                                    continue;
                                }
                            }
                            AstKind::UnaryExpression(unary)
                                if unary.operator == UnaryOperator::Delete =>
                            {
                                // `delete obj.prop` removes the property from the
                                // object and requires the member-expression form.
                                // `delete destructuredVar` would only delete the
                                // local binding, not the property, so this access
                                // cannot be rewritten — record the mutation and
                                // skip candidate collection.
                                if unary.argument.span().start == member.span().start
                                    && unary.argument.span().end == member.span().end
                                {
                                    mutated_props.insert(write_key(member, source, semantic));
                                    continue;
                                }
                            }
                            // Check grandparent for augmented assignment
                            _ => {
                                // Walk up further to check for augmented assignment
                                let gp_id = nodes.parent_id(parent_id);
                                if gp_id != parent_id {
                                    let gp = nodes.get_node(gp_id);
                                    if let AstKind::AssignmentExpression(assign) = gp.kind()
                                        && assign.left.span().start == member.span().start
                                            && assign.left.span().end == member.span().end
                                        {
                                            mutated_props.insert(write_key(member, source, semantic));
                                            continue;
                                        }
                                    // `delete (obj.prop)` — the member sits inside a
                                    // parenthesized argument of a delete. Same
                                    // mutation as the unparenthesized form.
                                    if let AstKind::UnaryExpression(unary) = gp.kind()
                                        && unary.operator == UnaryOperator::Delete
                                        && matches!(
                                            parent.kind(),
                                            AstKind::ParenthesizedExpression(_)
                                        )
                                    {
                                        mutated_props.insert(write_key(member, source, semantic));
                                        continue;
                                    }
                                }
                            }
                        }
                    }

                    let obj_text = &source[member.object.span().start as usize..member.object.span().end as usize];
                    let prop_text = member.property.name.as_str();

                    // Skip when this access sits in the initializer of a
                    // `const`/`let` whose binding name equals the property:
                    // destructuring that property from the same object would
                    // introduce a second block-scoped binding of the same name
                    // in the same scope (a redeclaration error), so the access
                    // cannot be mechanically rewritten to the destructured
                    // variable. A member expression can only reach a
                    // `VariableDeclarator` ancestor through its `init`, so a
                    // name match is sufficient.
                    let mut in_same_name_init = false;
                    for anc in nodes.ancestors(node.id()) {
                        if let AstKind::VariableDeclarator(decl) = anc.kind()
                            && let BindingPattern::BindingIdentifier(id) = &decl.id
                            && id.name.as_str() == prop_text
                        {
                            in_same_name_init = true;
                            break;
                        }
                    }
                    if in_same_name_init {
                        continue;
                    }

                    let obj_symbol = identifier_symbol(&member.object, semantic);

                    candidates.push(Candidate {
                        obj_text: obj_text.to_string(),
                        obj_symbol,
                        prop_text: prop_text.to_string(),
                        start_byte: member.span().start,
                    });
                }
                _ => {}
            }
        }

        // Phase 3: match candidates against destructured objects
        for c in &candidates {
            // A property that is written anywhere on this object cannot be
            // cached via destructuring without risking a stale value, so its
            // reads are never a style violation. Over-approximate over the
            // whole file rather than tracking strict before/after ordering:
            // mutations leak through nested closures and async IIFEs, where a
            // later read may execute *after* the write even though it appears
            // textually before it.
            if mutated_props.contains(&(
                c.obj_symbol,
                c.obj_text.clone(),
                c.prop_text.clone(),
            )) {
                continue;
            }
            for d in &destructured {
                if c.obj_text == d.obj_text && c.start_byte > d.end_byte {
                    // When both sides resolve to a binding, they must be the
                    // *same* binding. This rejects identically named variables
                    // declared in unrelated scopes (e.g. a loop variable and a
                    // sibling callback parameter that happen to share a name).
                    if let (Some(cand_sym), Some(decl_sym)) = (c.obj_symbol, d.obj_symbol)
                        && cand_sym != decl_sym
                    {
                        continue;
                    }
                    // The access must fall inside the destructuring's enclosing
                    // function AND, when the destructuring lives in a nested
                    // block, inside that block's span — the binding is only live
                    // there, so a sibling/ancestor block access has no
                    // destructured variable to be consistent with.
                    let fn_ok = match d.fn_range {
                        None => true,
                        Some((fn_start, fn_end)) => {
                            c.start_byte >= fn_start && c.start_byte <= fn_end
                        }
                    };
                    let block_ok = match d.block_range {
                        None => true,
                        Some((block_start, block_end)) => {
                            c.start_byte >= block_start && c.start_byte <= block_end
                        }
                    };
                    if fn_ok && block_ok {
                        let (line, column) = byte_offset_to_line_col(source, c.start_byte as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: "consistent-destructuring".into(),
                            message: format!(
                                "Use destructured variable for `{}` instead of `{}.{}`.",
                                c.prop_text, c.obj_text, c.prop_text,
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                        break;
                    }
                }
            }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn skips_cross_function_scope() {
        let code = r#"
            const obj = { x: 1, y: 2 };
            function first() {
                const { x } = obj;
                console.log(x);
            }
            function second() {
                console.log(obj.y);
            }
        "#;
        assert!(run(code).is_empty(), "Should not flag across function scopes");
    }

    #[test]
    fn flags_same_function_scope() {
        let code = r#"
            function test() {
                const { x } = obj;
                console.log(x);
                console.log(obj.y);
            }
        "#;
        let diags = run(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains('y'));
    }

    #[test]
    fn flags_nested_inner_scope() {
        let code = r#"
            function outer() {
                const { x } = obj;
                function inner() {
                    console.log(obj.y);
                }
            }
        "#;
        let diags = run(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains('y'));
    }

    #[test]
    fn skips_same_name_different_scopes() {
        // Issue #1224: a loop variable and a sibling callback parameter share
        // the name `sponsor` but are distinct bindings. Destructuring the loop
        // variable must not flag member access on the callback parameter.
        let code = r#"
            for (const sponsor of sponsors) {
                const { login } = sponsor;
                console.log(login);
            }
            const cols = buckets.map((sponsor) => {
                const imgSrc = new URL(sponsor.imgSrc);
                return sponsor.link;
            });
        "#;
        assert!(
            run(code).is_empty(),
            "Should not flag member access on a same-named binding in an unrelated scope"
        );
    }

    #[test]
    fn skips_read_of_mutated_property() {
        // Issue #2186: `state.asyncId` is assigned (mutated) after the
        // destructure of `state`, then read inside a nested closure. Reading
        // it directly is correct — destructuring upfront would capture the
        // pre-mutation (stale) value, so this is not a style violation.
        let code = r#"
            function runAsync(to, props, state, target) {
                const { callId, parentId, onRest } = props;
                const { asyncTo: prevTo, promise: prevPromise } = state;
                return (state.promise = (async () => {
                    state.asyncId = callId;
                    state.asyncTo = to;
                    const bailIfEnded = (bailSignal) => {
                        return (callId !== state.asyncId);
                    };
                })());
            }
        "#;
        assert!(
            run(code).is_empty(),
            "Should not flag a read of a property that is mutated in scope"
        );
    }

    #[test]
    fn skips_read_of_compound_assigned_property() {
        // Issue #2186: compound-assignment forms (`state.count += 1`) also
        // mutate the property, so a later read must not be flagged.
        let code = r#"
            function tick(state) {
                const { name } = state;
                state.count += 1;
                return state.count;
            }
        "#;
        assert!(
            run(code).is_empty(),
            "Should not flag a read of a property mutated via compound assignment"
        );
    }

    #[test]
    fn flags_read_of_never_assigned_property() {
        // Negative-space guard: a property that is never written is still the
        // rule's genuine target.
        let code = r#"
            function test(state) {
                const { x } = state;
                state.asyncId = 1;
                return state.foo;
            }
        "#;
        let diags = run(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("foo"));
    }

    #[test]
    fn write_on_other_object_does_not_exempt_read() {
        // Negative-space guard: a write to a different object must not exempt
        // a read of the same-named property on the destructured object.
        let code = r#"
            function test(state, other) {
                const { x } = state;
                other.bar = 1;
                return state.bar;
            }
        "#;
        let diags = run(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bar"));
    }

    #[test]
    fn skips_delete_of_destructured_property() {
        // Issue #6234: `delete obj.prop` removes the property from the object
        // and requires the member-expression form. `delete destructuredVar`
        // would only delete the local binding, not the property, so the access
        // cannot be rewritten to the destructured variable.
        let code = r#"
            const { createFastMessageEvent } = MessageEvent
            delete MessageEvent.createFastMessageEvent
        "#;
        assert!(
            run(code).is_empty(),
            "Should not flag `delete obj.prop` after destructuring from obj"
        );
    }

    #[test]
    fn skips_delete_of_parenthesized_destructured_property() {
        // Issue #6234, parenthesized form: `delete (obj.prop)` is the same
        // mutation as `delete obj.prop` and likewise cannot be rewritten.
        let code = r#"
            const { createFastMessageEvent } = MessageEvent
            delete (MessageEvent.createFastMessageEvent)
        "#;
        assert!(
            run(code).is_empty(),
            "Should not flag `delete (obj.prop)` after destructuring from obj"
        );
    }

    #[test]
    fn delete_skip_is_property_scoped() {
        // Negative control for #6234: deleting one property must not exempt a
        // read of a *different* destructured property — the delete-mutation is
        // keyed on the property, so the sibling read stays the rule's target.
        let code = r#"
            const { gone, kept } = obj
            delete obj.gone
            someFn(obj.kept)
        "#;
        let diags = run(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("kept"));
    }

    #[test]
    fn skips_sibling_block_scope() {
        // Issue #6176: `this` is destructured inside one `if`-block, then a
        // different property is read in a sibling `if`-block that the
        // destructuring's block does not lexically contain. The destructured
        // binding is not live at the read site, so it must not be flagged.
        let code = r#"
            function startOf(normalizedUnit, useLocaleWeeks, o) {
                if (normalizedUnit === "weeks") {
                    if (useLocaleWeeks) {
                        const { weekday } = this;
                        if (weekday < 1) {
                            o.weekNumber = 0;
                        }
                    }
                }
                if (normalizedUnit === "quarters") {
                    const q = Math.ceil(this.month / 3);
                    o.month = (q - 1) * 3 + 1;
                }
            }
        "#;
        assert!(
            run(code).is_empty(),
            "Should not flag a read in a sibling block where the destructured binding is out of scope"
        );
    }

    #[test]
    fn flags_hoisted_var_read_outside_block() {
        // `var` is function-scoped: a destructuring `var { x } = obj` inside a
        // block stays live after the block, so a later sibling-scope read is a
        // genuine inconsistency and must still be flagged.
        let code = r#"
            function test() {
                if (cond) {
                    var { x } = obj;
                }
                console.log(obj.y);
            }
        "#;
        let diags = run(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains('y'));
    }

    #[test]
    fn flags_same_block_scope() {
        // Negative-space guard: a read in the *same* block as the destructuring
        // (where the binding is live) is still a genuine inconsistency.
        let code = r#"
            function test() {
                if (cond) {
                    const { x } = obj;
                    console.log(obj.y);
                }
            }
        "#;
        let diags = run(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains('y'));
    }

    #[test]
    fn flags_nested_block_scope() {
        // Negative-space guard: a read in a descendant block (the binding is
        // still live there) is still flagged.
        let code = r#"
            function test() {
                if (cond) {
                    const { x } = obj;
                    if (other) {
                        console.log(obj.y);
                    }
                }
            }
        "#;
        let diags = run(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains('y'));
    }

    #[test]
    fn flags_same_binding_in_same_scope() {
        // Negative-space guard: a genuine within-scope inconsistency on the
        // very same binding is still flagged.
        let code = r#"
            function handle(sponsor) {
                const { login } = sponsor;
                console.log(login);
                console.log(sponsor.imgSrc);
            }
        "#;
        let diags = run(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("imgSrc"));
    }

    #[test]
    fn skips_access_in_same_name_const_init() {
        // Issue #6315: `args.format` / `args.target` appear in the
        // initializer of `const format = ...` / `const target = ...`.
        // Destructuring `format`/`target` from `args` would create a second
        // `const` of the same name in the same scope (a redeclaration error),
        // so these accesses must not be flagged.
        let code = r#"
            async function main(args) {
                const { input, output, outDir } = args;
                const format =
                    args.format === 'esm'
                        ? 'es'
                        : args.format || 'es';
                const target = args.target || 'es2020';
                return { input, output, outDir, format, target };
            }
        "#;
        assert!(
            run(code).is_empty(),
            "Should not flag an access in the initializer of a same-named const"
        );
    }

    #[test]
    fn flags_access_in_different_name_const_init() {
        // Negative-space guard: the declared name (`externals`) differs from
        // the property (`external`), so no redeclaration would occur and the
        // access stays flagged.
        let code = r#"
            function build(args) {
                const { input, output } = args;
                const externals = [].concat(args.external || []);
                return externals;
            }
        "#;
        let diags = run(code);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("external"));
    }
}

/// Resolve `expr` to the symbol of its binding when it is a plain identifier
/// reference. Returns `None` for `this`, member chains, or unresolved globals,
/// which have no single binding to key matching on.
fn identifier_symbol(expr: &Expression, semantic: &oxc_semantic::Semantic) -> Option<SymbolId> {
    let Expression::Identifier(ident) = expr else {
        return None;
    };
    let ref_id = ident.reference_id.get()?;
    semantic.scoping().get_reference(ref_id).symbol_id()
}

/// Build the mutation key for a write target `obj.prop`, matching the key used
/// for read candidates: the object's binding symbol, its source text, and the
/// property name.
fn write_key(
    member: &StaticMemberExpression,
    source: &str,
    semantic: &oxc_semantic::Semantic,
) -> (Option<SymbolId>, String, String) {
    let obj_text =
        &source[member.object.span().start as usize..member.object.span().end as usize];
    (
        identifier_symbol(&member.object, semantic),
        obj_text.to_string(),
        member.property.name.to_string(),
    )
}

fn is_simple_expression(expr: &Expression) -> bool {
    match expr {
        Expression::Identifier(_) | Expression::ThisExpression(_) => true,
        Expression::StaticMemberExpression(mem) => is_simple_expression(&mem.object),
        _ => false,
    }
}
