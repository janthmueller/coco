use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{DaemonMethod, DaemonRequest};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DoctorParams {}

impl DaemonRequest for DoctorParams {
    type Response = DoctorResult;
    const METHOD: DaemonMethod = DaemonMethod::Doctor;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DiagnosticStatus {
    Ok,
    Warning,
    Error,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticCheck {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) status: DiagnosticStatus,
    pub(crate) message: String,
    #[serde(default, skip_serializing_if = "super::is_false")]
    pub(crate) timed_out: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) subject_id: Option<String>,
}

impl DiagnosticCheck {
    pub(crate) fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        status: DiagnosticStatus,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            status,
            message: message.into(),
            timed_out: false,
            hint: None,
            subject: None,
            subject_id: None,
        }
    }

    pub(crate) fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub(crate) fn subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    pub(crate) fn subject_id(mut self, id: impl Into<String>) -> Self {
        self.subject_id = Some(id.into());
        self
    }

    pub(crate) fn timed_out(mut self) -> Self {
        self.timed_out = true;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DoctorDaemonInfo {
    pub(crate) version: String,
    pub(crate) pid: u32,
    pub(crate) executable: Option<PathBuf>,
    pub(crate) codex_binary: Option<PathBuf>,
    pub(crate) codex_version: Option<String>,
    pub(crate) codex_home: PathBuf,
    pub(crate) database_path: PathBuf,
    pub(crate) endpoint_path: PathBuf,
    pub(crate) token_path: PathBuf,
    pub(crate) execution_mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DoctorResult {
    pub(crate) daemon: DoctorDaemonInfo,
    pub(crate) checks: Vec<DiagnosticCheck>,
    pub(crate) complete: bool,
}
