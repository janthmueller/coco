use super::*;
use crate::protocol::{TurnGetParams, TurnInterruptMode, TurnInterruptParams};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TurnInput {
    /// Workspace name or ID belonging to this server's fixed repository.
    workspace: String,
    /// The caller-selected operationId used for workspaces.send.
    operation_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TurnInterruptInput {
    /// Workspace name or ID belonging to this server's fixed repository.
    workspace: String,
    /// The caller-selected operationId used for workspaces.send.
    operation_id: String,
    /// Strict by default. bestEffort explicitly accepts native cancellation races.
    #[serde(default)]
    #[schemars(schema_with = "interrupt_mode_schema")]
    mode: TurnInterruptMode,
}

fn interrupt_mode_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type": "string", "enum": TurnInterruptMode::ALL, "default": TurnInterruptMode::Strict})
}

#[tool_router(router = turn_tool_router, vis = "pub(super)")]
impl McpServer {
    #[tool(
        name = "turns.get",
        description = "Inspect one exact CoCo send operation and its technical turn outcome. Does not load conversation history or start execution.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn turns_get(&self, Parameters(input): Parameters<TurnInput>) -> CallToolResult {
        self.dispatcher.turns_get(input).await
    }

    #[tool(
        name = "turns.interrupt",
        description = "Request interruption of one stored send operation. Default strict requires atomic targeting; explicit mode bestEffort accepts that native Codex may interrupt a newer turn in a race. A saved attempt is never blindly re-sent. Only the original turn's interrupted outcome confirms its cancellation.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn turns_interrupt(
        &self,
        Parameters(input): Parameters<TurnInterruptInput>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let review = super::confirmation::interrupt_review(
            &self.dispatcher.repository,
            &input.workspace,
            &input.operation_id,
            input.mode,
        );
        if let Err(error) = self.confirm_change(review, &context).await {
            return error;
        }
        self.dispatcher.turns_interrupt(input).await
    }
}

impl<C: DaemonRpc> Dispatcher<C> {
    async fn turns_get(&self, input: TurnInput) -> CallToolResult {
        self.call(
            "turns.get",
            TurnGetParams {
                scope: RepositoryScope::repository(self.repository.clone()),
                workspace: input.workspace.clone(),
                operation_id: input.operation_id.clone(),
            },
            Some(input.workspace),
            Some(input.operation_id),
        )
        .await
    }

    async fn turns_interrupt(&self, input: TurnInterruptInput) -> CallToolResult {
        self.call(
            "turns.interrupt",
            TurnInterruptParams {
                scope: RepositoryScope::repository(self.repository.clone()),
                workspace: input.workspace.clone(),
                operation_id: input.operation_id.clone(),
                mode: input.mode,
            },
            Some(input.workspace),
            Some(input.operation_id),
        )
        .await
    }
}

#[cfg(test)]
mod tests;
