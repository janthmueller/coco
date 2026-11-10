use super::*;
use rmcp::model::{ElicitRequestParams, ElicitResult, ElicitationAction, ElicitationSchema};
use rmcp::service::ElicitationMode;
use std::path::Path;
use std::time::Duration;

mod review;
pub(super) use review::{create_review, interrupt_review, send_review};

impl McpServer {
    /// An opt-in host confirmation, never a model-supplied tool parameter.
    /// No coordinator mutation occurs until this exact invocation is accepted.
    pub(super) async fn confirm_change(
        &self,
        review: String,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), CallToolResult> {
        if !self.confirm_changes {
            return Ok(());
        }
        if review.len() > 16 * 1024 {
            return Err(failure(
                "CONFIRMATION_TOO_LARGE",
                "Action review exceeds 16 KiB; use an ordinary CoCo command",
            ));
        }
        if !context
            .peer
            .supported_elicitation_modes()
            .contains(&ElicitationMode::Form)
        {
            return Err(failure(
                "CONFIRMATION_UNAVAILABLE",
                "The connected host does not support action confirmation",
            ));
        }
        let schema = ElicitationSchema::builder()
            .required_bool_property("confirm", |value| {
                value.title("Run this action?").with_default(false)
            })
            .build()
            .map_err(|_| {
                failure(
                    "CONFIRMATION_UNAVAILABLE",
                    "Action review schema is unavailable",
                )
            })?;
        let request = ElicitRequestParams::FormElicitationParams {
            meta: None,
            message: review,
            requested_schema: schema,
        };
        let response = tokio::select! {
            biased;
            () = context.ct.cancelled() => return Err(failure("CONFIRMATION_CANCELLED", "Action was cancelled before dispatch")),
            response = context.peer.create_elicitation_with_timeout(request, Some(Duration::from_secs(300))) => response,
        };
        let response = response.map_err(|_| {
            failure(
                "CONFIRMATION_UNAVAILABLE",
                "Action confirmation was not received; nothing was dispatched",
            )
        })?;
        if context.ct.is_cancelled() || !accepted(&response) {
            return Err(failure(
                "CONFIRMATION_CANCELLED",
                "Action was not confirmed; nothing was dispatched",
            ));
        }
        Ok(())
    }
}

fn accepted(result: &ElicitResult) -> bool {
    result.action == ElicitationAction::Accept
        && result.content.as_ref() == Some(&json!({"confirm": true}))
}

fn failure(code: &str, message: &str) -> CallToolResult {
    CallToolResult::structured_error(DaemonFailure::new(code, message).as_json())
}

/// Keep data on one line and prevent terminal/control-sequence injection or
/// forged review labels. Escaping changes presentation, never dispatched data.
fn field(label: &str, value: &str) -> String {
    let value = value
        .chars()
        .map(|value| {
            if value.is_control() {
                value.escape_default().to_string()
            } else {
                value.to_string()
            }
        })
        .collect::<String>();
    format!("{label:<13}{value}\n")
}

fn repository(path: &Path) -> String {
    field("Repository", &path.to_string_lossy())
}

#[cfg(test)]
mod tests;
