//! Evidence-based native contracts shared without importing transport adapters.

pub(crate) const VERIFIED_VERSIONS: &[&str] = &["0.166.0", "0.169.0", "0.171.0"];

pub(crate) fn is_verified(version: Option<&str>) -> bool {
    version.is_some_and(|version| VERIFIED_VERSIONS.contains(&version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicitly_verified_release_builds_enable_sensitive_contracts() {
        for version in VERIFIED_VERSIONS {
            assert!(is_verified(Some(version)));
        }
        for version in [
            None,
            Some(""),
            Some("0.165.0"),
            Some("0.167.0"),
            Some("0.170.0"),
            Some("0.171.1"),
            Some("0.172.0"),
            Some("0.171.0-alpha.1"),
            Some("0.171.0-custom"),
            Some("codex-cli 0.171.0"),
        ] {
            assert!(!is_verified(version), "unverified release: {version:?}");
        }
    }
}
