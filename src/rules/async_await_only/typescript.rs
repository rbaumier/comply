#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::async_await_only::oxc_typescript::Check;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_then_chain() {
        let diags = run("fetchUser(id).then(data => { console.log(data); });");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains(".then()"));
    }

    #[test]
    fn flags_catch_chain() {
        let diags = run("fetchUser(id).catch(err => { console.error(err); });");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains(".catch()"));
    }

    #[test]
    fn flags_then_and_catch() {
        let diags = run("fetchUser(id).then(d => d).catch(e => e);");
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn allows_await() {
        assert!(run("async function f() { const d = await fetchUser(id); }").is_empty());
    }

    #[test]
    fn allows_array_then() {
        assert!(run("const arr = [1, 2]; arr.map(x => x);").is_empty());
    }
}
