//! no-await-in-loop OXC backend — flag `await` inside a loop body, but
//! exempt recursive calls to the enclosing async function (deliberate
//! depth-first traversal).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, PropertyKey};
use std::sync::Arc;

pub struct Check;

/// Extract the identifier name of the call target, if the awaited
/// expression is a direct call to an identifier or a `this.method` call.
/// `obj.method()` is NOT treated as self-recursion — only `this.method()` is.
fn awaited_callee_name<'a>(arg: &Expression<'a>) -> Option<&'a str> {
    let Expression::CallExpression(call) = arg else {
        return None;
    };
    match &call.callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => {
            if matches!(member.object, Expression::ThisExpression(_)) {
                Some(member.property.name.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Walk ancestors of the `await` looking for a loop boundary. Stops at
/// function boundaries (a nested `async` function starts a fresh
/// context — its awaits are not "in" the outer loop). Returns the name
/// of the enclosing async function when a loop is found, so the caller
/// can compare against the awaited callee for recursion detection.
///
/// Return values:
///   - `Some(Some(name))` — inside a loop in a named async function
///   - `Some(None)` — inside a loop in an unnamed/arrow async function
///   - `None` — not inside a loop (or the enclosing function is reached first)
fn enclosing_loop_and_fn_name<'a>(
    node_id: oxc_semantic::NodeId,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<Option<&'a str>> {
    let nodes = semantic.nodes();
    let mut current_id = node_id;
    let mut saw_loop = false;
    loop {
        let parent_id = nodes.parent_id(current_id);
        if parent_id == current_id {
            return None;
        }
        let parent = nodes.get_node(parent_id);
        match parent.kind() {
            AstKind::ForStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::DoWhileStatement(_) => {
                saw_loop = true;
            }
            AstKind::Function(func) => {
                if !saw_loop {
                    return None;
                }
                // Named function declarations/expressions have their own id.
                if let Some(id) = &func.id {
                    return Some(Some(id.name.as_str()));
                }
                // Class methods (`async method() {}`) have no func.id — the
                // name lives on the parent MethodDefinition's key.
                let gp_id = nodes.parent_id(parent_id);
                if gp_id != parent_id {
                    if let AstKind::MethodDefinition(method) = nodes.get_node(gp_id).kind() {
                        if let PropertyKey::StaticIdentifier(id) = &method.key {
                            return Some(Some(id.name.as_str()));
                        }
                    }
                }
                return Some(None);
            }
            AstKind::ArrowFunctionExpression(_) => {
                if !saw_loop {
                    return None;
                }
                // Arrow functions are nameless at the syntax level. Try to
                // recover the conventional name from the parent binding.
                let gp_id = nodes.parent_id(parent_id);
                if gp_id != parent_id {
                    let gp_kind = nodes.get_node(gp_id).kind();
                    // `const foo = async () => {}` — VariableDeclarator binding.
                    if let AstKind::VariableDeclarator(decl) = gp_kind
                        && let BindingPattern::BindingIdentifier(id) = &decl.id
                    {
                        return Some(Some(id.name.as_str()));
                    }
                    // `foo = async () => {}` as a class property — PropertyDefinition key.
                    if let AstKind::PropertyDefinition(prop) = gp_kind
                        && let PropertyKey::StaticIdentifier(id) = &prop.key
                    {
                        return Some(Some(id.name.as_str()));
                    }
                }
                return Some(None);
            }
            _ => {}
        }
        current_id = parent_id;
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::AwaitExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::AwaitExpression(await_expr) = node.kind() else {
            return;
        };

        let Some(enclosing_fn_name) = enclosing_loop_and_fn_name(node.id(), semantic) else {
            return;
        };

        // Recursion exception: if the awaited expression is a direct
        // call to the enclosing async function, skip — sequential
        // recursion is the only way to express depth-first traversal.
        if let (Some(fn_name), Some(callee)) =
            (enclosing_fn_name, awaited_callee_name(&await_expr.argument))
            && fn_name == callee
        {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, await_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Sequential `await` in a loop serializes independent work. \
                      If the iterations don't depend on each other, use \
                      `Promise.all(items.map(f))` instead."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::no_await_in_loop::oxc_typescript::Check;
    use crate::rules::test_helpers::run_oxc_ts;



    fn run(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_await_in_for_of_loop() {
        let src = r"
            async function fetchAll(urls: string[]) {
                for (const url of urls) {
                    const r = await fetch(url);
                }
            }
        ";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_await_in_for_loop() {
        let src = r"
            async function run(n: number) {
                for (let i = 0; i < n; i++) {
                    await step(i);
                }
            }
        ";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_await_in_while_loop() {
        let src = r"
            async function drain(q: any) {
                while (q.has()) {
                    await q.pop();
                }
            }
        ";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_await_in_do_while_loop() {
        let src = r"
            async function poll() {
                do {
                    await tick();
                } while (running);
            }
        ";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_await_in_for_in_loop() {
        let src = r"
            async function each(obj: Record<string, string>) {
                for (const k in obj) {
                    await write(k);
                }
            }
        ";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn ignores_await_outside_loop() {
        let src = r"
            async function once() {
                await fetch('/x');
            }
        ";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_await_in_promise_all_map() {
        let src = r"
            async function fanout(urls: string[]) {
                await Promise.all(urls.map(async (u) => await fetch(u)));
            }
        ";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_recursive_await_in_for_of_named_fn() {
        let src = r"
            async function collectHandlerFiles(dir: string, into: string[]): Promise<void> {
                const entries = await readdir(dir, { withFileTypes: true });
                for (const entry of entries) {
                    if (entry.isDirectory()) {
                        await collectHandlerFiles(entry.name, into);
                        continue;
                    }
                    into.push(entry.name);
                }
            }
        ";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_recursive_await_in_arrow_fn() {
        let src = r"
            const walk = async (dir: string): Promise<void> => {
                const entries: any[] = [];
                for (const entry of entries) {
                    await walk(entry);
                }
            };
        ";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_non_recursive_await_inside_recursive_fn() {
        let src = r"
            async function walk(dir: string): Promise<void> {
                const entries: any[] = [];
                for (const entry of entries) {
                    await sideEffect(entry);
                }
            }
        ";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_member_call_on_non_this_receiver_same_name_as_fn() {
        let src = r"
            async function process(obj: any) {
                for (const item of obj.items) {
                    await obj.process(item);
                }
            }
        ";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_await_in_nested_async_fn_inside_loop() {
        let src = r"
            async function outer(urls: string[]) {
                for (const url of urls) {
                    const fetcher = async () => {
                        return await fetch(url);
                    };
                    fetcher();
                }
            }
        ";
        // The arrow's body's await is inside a loop body but bounded by
        // the arrow function — it doesn't serialize the outer loop.
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_recursive_await_in_class_method() {
        let src = r"
            class TreeWalker {
                async traverse(nodes: ASTNode[]): Promise<void> {
                    for (const node of nodes) {
                        await this.traverse(node.children);
                    }
                }
            }
        ";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_await_this_other_method_in_class_loop() {
        let src = r"
            class Processor {
                async processAll(items: Item[]): Promise<void> {
                    for (const item of items) {
                        await this.processItem(item);
                    }
                }
            }
        ";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_recursive_await_in_class_property_arrow() {
        let src = r"
            class TreeWalker {
                traverse = async (nodes: ASTNode[]): Promise<void> => {
                    for (const node of nodes) {
                        await this.traverse(node.children);
                    }
                };
            }
        ";
        assert!(run(src).is_empty());
    }
}
