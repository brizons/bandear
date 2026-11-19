//! The bindings the iOS and Android apps use to call `bandear-core`.
//!
//! UniFFI generates the Swift and Kotlin sides from the `#[uniffi::export]` items here, so this
//! crate holds only the binding surface: thin wrappers that turn `bandear-core` types into types
//! UniFFI can carry, plus the hand-written JNI functions Android's receive pipeline uses instead
//! (`receive_pipeline_jni`). The logic stays in `bandear-core`, which knows nothing about either
//! and is tested on the host on its own. The server and the desktop app depend on neither crate.

uniffi::setup_scaffolding!();

// The control path: the desk's messages read, the phone's written, the reconnect policy, the
// client-id rule and the saved mixes' JSON.
mod control;
// The musician's mix: the session's mix and the saved mixes, one `MixModel` per phone.
mod mix;
mod receive_pipeline;
// Android calls the receive pipeline through hand-written JNI functions instead of UniFFI's
// binding, which costs too much per packet there; see the module.
#[cfg(not(target_os = "ios"))]
mod receive_pipeline_jni;

/// The version of the Rust core the app is running, for diagnostics and to prove the bindings are
/// wired end to end. This crate's version is kept equal to `bandear-core`'s (a test checks it), so
/// `bandear-core` needs no version constant of its own.
#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `version = "..."` from `bandear-core`'s manifest: the version the apps must see.
    fn core_manifest_version() -> &'static str {
        include_str!("../../bandear-core/Cargo.toml")
            .lines()
            .find_map(|line| line.strip_prefix("version = \""))
            .and_then(|rest| rest.strip_suffix('"'))
            .expect("bandear-core's Cargo.toml has a version line")
    }

    #[test]
    fn core_version_is_the_version_of_bandear_core() {
        assert_eq!(core_version(), core_manifest_version());
    }
}
