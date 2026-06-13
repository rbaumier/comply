#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::no_interface_prefix_i::oxc_typescript::Check;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_i_prefix() {
        let diags = run("interface IUserRepository {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("UserRepository"));
    }

    #[test]
    fn flags_exported_i_prefix() {
        let diags = run("export interface IService {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_normal_interface() {
        assert!(run("interface UserRepository {}").is_empty());
    }

    #[test]
    fn allows_lowercase_after_i() {
        assert!(run("interface Item {}").is_empty());
    }

    #[test]
    fn allows_single_letter() {
        assert!(run("interface I {}").is_empty());
    }

    #[test]
    fn flags_i_prefix_with_extends() {
        let diags = run("interface IProps extends BaseProps {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn ignores_type_alias() {
        assert!(run("type IFoo = { x: number };").is_empty());
    }

    #[test]
    fn allows_initialism_ip_rule() {
        assert!(run("interface IPRule {}").is_empty());
    }

    #[test]
    fn allows_initialism_ipv4_settings() {
        assert!(run("interface IPv4FirewallSettings {}").is_empty());
    }

    #[test]
    fn allows_initialism_ipv4_rule() {
        assert!(run("interface IPv4FirewallRule {}").is_empty());
    }

    #[test]
    fn allows_initialism_io_stream() {
        assert!(run("interface IOStream {}").is_empty());
    }

    #[test]
    fn flags_i_on_foo() {
        let diags = run("interface IFoo {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Foo"));
    }

    #[test]
    fn flags_i_on_logger_callback() {
        let diags = run("interface ILoggerCallback {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("LoggerCallback"));
    }

    #[test]
    fn flags_i_on_order() {
        let diags = run("interface IOrder {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Order"));
    }
}
