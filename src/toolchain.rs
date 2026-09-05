//! Mandatory external toolchains.
//!
//! comply delegates part of its rule set to programs it does not own: clippy,
//! cargo-shear, cargo-modules, oxlint, and the typescript-go checker behind the
//! type-aware sidecar. None is optional. Skipping one would silently
//! under-report, so comply refuses to run rather than print a false "clean".
//!
//! A refusal is a diagnosis, not a crash. This type is what every refusal
//! raises, so `main` can answer with the remedy instead of a bug-report banner.

/// A toolchain comply requires but cannot use.
#[derive(Debug, thiserror::Error)]
pub enum ToolchainError {
    /// Absent, and one command installs it.
    #[error("{what}.\nInstall it with: {install_cmd}")]
    Missing { what: String, install_cmd: String },

    /// A gap comply can name but no single command closes.
    /// An interpreter with no one-line install, a checker API that won't start.
    #[error("{what}.")]
    Unusable { what: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_renders_the_install_command_on_its_own_line() {
        let err = ToolchainError::Missing {
            what: "oxlint is required but was not found".to_owned(),
            install_cmd: "npm install -g oxlint".to_owned(),
        };
        assert_eq!(
            err.to_string(),
            "oxlint is required but was not found.\nInstall it with: npm install -g oxlint"
        );
    }

    #[test]
    fn unusable_renders_a_single_sentence() {
        let err = ToolchainError::Unusable { what: "`node` was not found".to_owned() };
        assert_eq!(err.to_string(), "`node` was not found.");
    }
}
