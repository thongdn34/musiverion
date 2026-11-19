//! Domain model and studio-language operations.
//!
//! `mv-core` defines the types, the traits (`GitBackend`, `AudioAnalyzer`,
//! `ProjectParser`, `Clock`) and the rules (lock policy, conflict-copy policy,
//! naming) that the rest of the workspace implements. It performs no I/O, so
//! every rule is unit-testable with fakes.
//!
//! Status: scaffold. Only [`AppInfo`] exists so far.

/// Name and version of the Musiverion core engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    /// Product name.
    pub name: String,
    /// Semantic version of the core crate.
    pub version: String,
}

impl AppInfo {
    /// Information about this build of the core engine.
    pub fn current() -> Self {
        Self {
            name: "Musiverion".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_reports_crate_version() {
        let info = AppInfo::current();
        assert_eq!(info.name, "Musiverion");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
    }
}
