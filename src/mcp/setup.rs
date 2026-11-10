use super::*;
use crate::protocol::{WorkspaceCreationGetParams, WorkspaceSetupParams, WorkspaceSetupRequest};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateInput {
    #[serde(flatten)]
    #[schemars(schema_with = "setup_schema")]
    request: WorkspaceSetupRequest,
}

fn setup_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object", "additionalProperties": false,
        "required": ["operationId", "name", "worktree", "changes", "profile"],
        "properties": {
            "operationId": {"type": "string", "minLength": 1, "maxLength": 256},
            "name": {"type": "string", "description": "A new repository-local workspace name."},
            "worktree": {"oneOf": [
                {"type": "object", "additionalProperties": false, "required": ["kind", "base"], "properties": {
                    "kind": {"const": "newBranch"}, "branch": {"type": "string"}, "base": base_schema()
                }},
                {"type": "object", "additionalProperties": false, "required": ["kind", "base"], "properties": {
                    "kind": {"const": "detached"}, "base": base_schema()
                }},
                {"type": "object", "additionalProperties": false, "required": ["kind", "branch"], "properties": {
                    "kind": {"const": "existingBranch"}, "branch": {"type": "string"}
                }}
            ]},
            "changes": {"type": "string", "enum": ["reject", "ignore", "carryTracked", "carryTrackedAndUntracked"]},
            "profile": {"type": "string", "description": "Execution profile, explicitly default for no overlay."},
            "context": {"oneOf": [
                {"type": "object", "additionalProperties": false, "required": ["kind"], "properties": {
                    "kind": {"const": "fresh"}
                }},
                {"type": "object", "additionalProperties": false, "required": ["kind", "workspace"], "properties": {
                    "kind": {"const": "fork"}, "workspace": {"type": "string", "minLength": 1,
                        "description": "Source workspace name or ID in this repository. Code base is selected independently."},
                    "compact": {"type": "boolean", "default": false}
                }}
            ]},
            "model": {"type": ["string", "null"]},
            "workerMcp": {"type": ["object", "null"], "additionalProperties": false, "required": ["path", "sha256"], "properties": {
                "path": {"type": "string", "description": "Canonical absolute private local JSON file, at most 64 KiB."},
                "sha256": {"type": "string", "pattern": "^[a-f0-9]{64}$", "description": "SHA-256 of the exact file bytes."}
            }}
        }
    })
}

fn base_schema() -> Value {
    json!({"oneOf": [
        {"type": "object", "additionalProperties": false, "required": ["kind", "revision"], "properties": {
            "kind": {"const": "revision"}, "revision": {"type": "string"}
        }},
        {"type": "object", "additionalProperties": false, "required": ["kind", "workspace"], "properties": {
            "kind": {"const": "workspace"}, "workspace": {"type": "string"}
        }}
    ]})
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreationGetInput {
    /// The original caller-selected creation operationId.
    operation_id: String,
}

#[tool_router(router = setup_tool_router, vis = "pub(super)")]
impl McpServer {
    #[tool(
        name = "workspaces.create",
        description = "Create a permanent workspace. Context defaults to fresh; context.kind=fork captures the source workspace's last finished turn now, independently of the Git base. Sources must belong to this repository. Optional compact runs on the child before its first activation. Requires --allow-create. Reuse the same operationId after a lost reply; changed requests conflict. Inspect preparing/unknown results instead of recreating.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn workspaces_create(
        &self,
        Parameters(input): Parameters<CreateInput>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let review =
            super::confirmation::create_review(&self.dispatcher.repository, &input.request);
        if let Err(error) = self.confirm_change(review, &context).await {
            return error;
        }
        self.dispatcher.create_workspace(input).await
    }

    #[tool(
        name = "workspaceCreations.get",
        description = "Read the durable receipt for a creation operation in this repository. Unknown is not proof that no workspace was created. Configured confirms saved setup, not an initialized MCP connection or model execution.",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_creations_get(
        &self,
        Parameters(input): Parameters<CreationGetInput>,
    ) -> CallToolResult {
        self.dispatcher.get_creation(input).await
    }
}

impl<C: DaemonRpc> Dispatcher<C> {
    async fn create_workspace(&self, input: CreateInput) -> CallToolResult {
        let operation_id = input.request.operation_id.clone();
        self.call(
            "workspaces.create",
            WorkspaceSetupParams {
                repository_path: self.repository.clone(),
                request: input.request,
            },
            None,
            Some(operation_id),
        )
        .await
    }

    async fn get_creation(&self, input: CreationGetInput) -> CallToolResult {
        self.call(
            "workspaceCreations.get",
            WorkspaceCreationGetParams {
                repository_path: self.repository.clone(),
                operation_id: input.operation_id.clone(),
            },
            None,
            Some(input.operation_id),
        )
        .await
    }
}

#[cfg(test)]
mod tests;
