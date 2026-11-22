pub(crate) const SELECTED_CODEX_VERSION: &str = "codex-cli 0.171.0";

pub(crate) fn expected_version() -> String {
    std::env::var("COCO_REAL_CODEX_VERSION")
        .map(|version| format!("codex-cli {version}"))
        .unwrap_or_else(|_| SELECTED_CODEX_VERSION.to_owned())
}
