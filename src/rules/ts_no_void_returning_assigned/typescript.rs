//! Tests for ts-no-void-returning-assigned.

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::test_helpers::run_rule_by_id;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_rule_by_id("ts-no-void-returning-assigned", source, "t.ts")
    }

    #[test]
    fn flags_const_console_log() {
        let src = "const x = console.log('hi');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_const_foreach() {
        let src = "const r = arr.forEach(x => x + 1);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_const_map() {
        let src = "const r = arr.map(x => x + 1);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_console_log() {
        let src = "console.log('hi');";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_let_console_error() {
        let src = "let y = console.error('boom');";
        assert_eq!(run(src).len(), 1);
    }

    // Regression for #3876: an arrow function whose concise body is a void call
    // holds a function, never `undefined`.
    #[test]
    fn allows_arrow_with_foreach_body() {
        let src = "const notify = () => callbacks.forEach((fn) => fn());";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arrow_single_param_foreach_body() {
        let src = "const f = a => a.forEach(g);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arrow_with_console_log_body() {
        let src = "const logIt = () => console.log('hi');";
        assert!(run(src).is_empty());
    }

    // The void value is genuinely assigned: the callback arrow is nested inside
    // `.map(...)`, and `.forEach(...)` is the direct init of the binding.
    #[test]
    fn flags_chained_map_then_foreach() {
        let src = "const x = items.map(y => y.z).forEach(f);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_reassignment_to_foreach() {
        let src = "x = arr.forEach(f);";
        assert_eq!(run(src).len(), 1);
    }
}
