//! The program's environment variables (not in the original).
//!
//! Each setting has a name like `USER_DIR` and is read from `UNBANISHED_USER_DIR`. The prefix is
//! in one place, here.

/// The value of `UNBANISHED_<name>`.
pub fn var(name: &str) -> Result<String, std::env::VarError> {
    std::env::var(format!("UNBANISHED_{name}"))
}

/// [`var`] as an `OsString`, for paths.
pub fn var_os(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(format!("UNBANISHED_{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_unbanished_name() {
        std::env::set_var("UNBANISHED_ENV_TEST_A", "set");
        assert_eq!(var("ENV_TEST_A").as_deref(), Ok("set"));
        assert_eq!(
            var_os("ENV_TEST_A").as_deref(),
            Some(std::ffi::OsStr::new("set"))
        );
        assert!(var("ENV_TEST_NEITHER").is_err());
    }
}
