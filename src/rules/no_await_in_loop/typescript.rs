//! no-await-in-loop tests.

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::no_await_in_loop::oxc_typescript::Check;
    
    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    /// Regression for #1182 — `await Promise.all(batch.map(...))` inside a
    /// loop is a batching pattern (each batch parallelized, batches
    /// sequential for back-pressure). Must not be flagged.
    #[test]
    fn ignores_await_promise_all_in_loop() {
        let src = r"
            async function processInBatches(batches: string[][]) {
                for (const batch of batches) {
                    await Promise.all(batch.map(item => fetch(item)));
                }
            }
        ";
        assert!(run(src).is_empty());
    }

    /// `Promise.allSettled`, `Promise.race`, and `Promise.any` are equally
    /// explicit multi-promise coordination — awaiting one in a loop is exempt.
    #[test]
    fn ignores_await_other_promise_combinators_in_loop() {
        let settled = r"
            async function run(batches: string[][]) {
                for (const batch of batches) {
                    await Promise.allSettled(batch.map(item => fetch(item)));
                }
            }
        ";
        let race = r"
            async function run(rounds: Promise<unknown>[][]) {
                for (const round of rounds) {
                    await Promise.race(round);
                }
            }
        ";
        let any = r"
            async function run(rounds: Promise<unknown>[][]) {
                for (const round of rounds) {
                    await Promise.any(round);
                }
            }
        ";
        assert!(run(settled).is_empty());
        assert!(run(race).is_empty());
        assert!(run(any).is_empty());
    }

    /// Negative space for #1182 — the batching exemption is scoped to the
    /// `Promise` combinators. A genuine single-promise `await` per iteration
    /// is still the serial anti-pattern and must fire exactly one diagnostic.
    #[test]
    fn flags_single_await_alongside_promise_all_exemption() {
        let src = r"
            async function run(urls: string[]) {
                for (const url of urls) {
                    await fetch(url);
                }
            }
        ";
        assert_eq!(run(src).len(), 1);
    }

    /// Regression for #104 — deliberate sequential async recursion in a
    /// depth-first directory walk must not be flagged. The await target
    /// is a recursive call to the enclosing async function.
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

    /// Recursion exemption also applies to async arrow functions
    /// assigned to a const binding (the conventional name).
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

    /// A call to a *different* function inside a recursive walk is
    /// still flagged — only self-recursion is the documented exception.
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

    /// Regression: `await obj.process()` inside `async function process()` is NOT
    /// self-recursion — the receiver is `obj`, not `this`. Must still flag.
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

    /// `await` inside a nested async function that itself sits inside a
    /// loop must not be attributed to the loop — the inner function is
    /// a fresh async context.
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

    /// Regression for #366 — class method self-recursion via `this.method()`
    /// inside a loop must be exempt. `func.id` is None for class methods; the
    /// name must be recovered from the parent MethodDefinition.
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

    /// Class method calling a *different* method must still be flagged —
    /// only self-recursion is exempt.
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

    /// Regression for #1510 — an `await` in the `for-of` iterable expression
    /// is evaluated exactly once before iteration, not per iteration, so it is
    /// not a serial-await-in-loop and must not be flagged.
    #[test]
    fn ignores_await_in_for_of_iterable_expression() {
        let src = r#"
            async function index(text: string) {
                const hashedRefs = new Map<string, string>();
                for (const [line, _] of (await shiki).splitLines(text)) {
                    const [hash, name] = line.split(" ");
                    hashedRefs.set(name, hash);
                }
            }
        "#;
        assert!(run(src).is_empty());
    }

    /// Companion to #1510 — an `await` in the `for-in` object expression is
    /// likewise evaluated once before iteration begins.
    #[test]
    fn ignores_await_in_for_in_object_expression() {
        let src = r"
            async function each() {
                for (const k in await loadConfig()) {
                    use(k);
                }
            }
        ";
        assert!(run(src).is_empty());
    }

    /// Companion to #1510 — a C-style `for(;;)` `init` clause runs exactly
    /// once, so an `await` there is not per-iteration.
    #[test]
    fn ignores_await_in_for_init_clause() {
        let src = r"
            async function run() {
                for (let i = await start(); i < 10; i++) {
                    use(i);
                }
            }
        ";
        assert!(run(src).is_empty());
    }

    /// Negative space for #1510 — the iterable-header exemption must not leak
    /// into the loop body. An `await` in the body still serializes work and
    /// must fire even when the iterable expression also contains one.
    #[test]
    fn flags_await_in_body_even_with_await_in_iterable() {
        let src = r"
            async function run(items: any) {
                for (const x of await fetchAll()) {
                    await f(x);
                }
            }
        ";
        assert_eq!(run(src).len(), 1);
    }

    /// Negative space for #1510 — a `for(;;)` `test`/`update` runs per
    /// iteration, so an `await` there is still the serial anti-pattern.
    #[test]
    fn flags_await_in_for_test_clause() {
        let src = r"
            async function run() {
                for (let i = 0; await more(i); i++) {
                    use(i);
                }
            }
        ";
        assert_eq!(run(src).len(), 1);
    }

    /// Regression for #366 — class property arrow function self-recursion via
    /// `this.method()` must be exempt (PropertyDefinition key recovery).
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
