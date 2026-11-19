//! Application service.
//!
//! The single API used by every adapter: the Tauri bridge (`apps/desktop`),
//! the companion server (`mv-remote`) and the CLI (`mv-cli`). It will own
//! jobs, progress events and orchestration over the other crates. Adapters
//! hold no business logic.
//!
//! Status: scaffold. Only [`ping`] exists so far.

use serde::Serialize;

/// Reply to [`ping`]: proves an adapter reaches the application service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PingReply {
    /// Human-readable greeting.
    pub message: String,
    /// Version of `mv-core` this build links against.
    pub core_version: String,
}

/// Health check for adapters.
pub fn ping() -> PingReply {
    let info = mv_core::AppInfo::current();
    PingReply {
        message: format!("{} is running", info.name),
        core_version: info.version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_reports_core_version_in_camel_case() {
        let reply = ping();
        assert_eq!(reply.core_version, mv_core::AppInfo::current().version);
        let json = serde_json::to_value(&reply).unwrap();
        assert!(json.get("coreVersion").is_some());
    }
}
