//! Optional native footer. Ordinary control tools retain their own contracts.

use std::time::{Duration, Instant};

use rmcp::model::{
    ListResourcesResult, ReadResourceResult, RequestMetaObject, Resource, ResourceContents,
};
use tokio::sync::Mutex;

use super::*;
use crate::protocol::{
    RepositoryResolveParams, WorkspaceStatuslineParams, WorkspaceStatuslineSnapshot,
};

mod render;
#[cfg(test)]
mod tests;

const URI: &str = "ui://coco/workspace-statusline";
const STATE_META: &str = "com.openai/codex-statusline-state";
const CACHE_TTL: Duration = Duration::from_millis(1500);
const READ_BUDGET: Duration = Duration::from_millis(700);

pub(super) fn advertise(router: &mut ToolRouter<McpServer>) {
    if let Some(route) = router.map.get_mut(WORKSPACES_STATUS) {
        route.attr.title = Some("CoCo workspace".into());
        route.attr.meta = Some(
            json!({"openai/ui": {"entrypoints": [
                {"type":"statusline", "resourceUri": URI}
            ]}})
            .as_object()
            .expect("literal object")
            .clone()
            .into(),
        );
    }
}

pub(super) fn resources() -> ListResourcesResult {
    let mut result = ListResourcesResult::default();
    result.resources.push(
        Resource::new(URI, "workspace-statusline")
            .with_title("CoCo workspace")
            .with_description("Passive workspace and native Codex footer for the displayed thread.")
            .with_mime_type("application/json"),
    );
    result
}

#[derive(Default)]
pub(super) struct Provider {
    cache: Mutex<Cache>,
}

#[derive(Default)]
struct Cache {
    // Pinned from the host-selected repository, never from client metadata.
    repository_id: Option<String>,
    retry_after: Option<Instant>,
    last: Option<Cached>,
}

struct Cached {
    thread: String,
    at: Instant,
    value: Option<WorkspaceStatuslineSnapshot>,
}

impl Provider {
    pub(super) async fn read<C: DaemonRpc>(
        &self,
        dispatcher: &Dispatcher<C>,
        uri: &str,
        meta: &RequestMetaObject,
    ) -> Result<ReadResourceResult, rmcp::ErrorData> {
        if uri != URI {
            return Err(rmcp::ErrorData::resource_not_found(
                "unknown CoCo resource",
                None,
            ));
        }
        let state = meta.get(STATE_META).unwrap_or(&Value::Null);
        let thread = state
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty() && id.len() <= 256 && !id.chars().any(char::is_control));
        let workspace = if let Some(thread) = thread {
            tokio::time::timeout(READ_BUDGET, self.snapshot(dispatcher, thread))
                .await
                .ok()
                .flatten()
        } else {
            None
        };
        let tree = render::footer(state, workspace.as_ref());
        let text = serde_json::to_string(&tree).expect("bounded literal UI tree");
        Ok(ReadResourceResult::new(vec![
            ResourceContents::text(text, URI).with_mime_type("application/json"),
        ]))
    }

    async fn snapshot<C: DaemonRpc>(
        &self,
        dispatcher: &Dispatcher<C>,
        thread: &str,
    ) -> Option<WorkspaceStatuslineSnapshot> {
        let mut cache = self.cache.lock().await;
        if let Some(last) = &cache.last
            && last.thread == thread
            && last.at.elapsed() < CACHE_TTL
        {
            return last.value.clone();
        }
        if cache.repository_id.is_none() {
            if cache.retry_after.is_some_and(|at| at > Instant::now()) {
                return None;
            }
            // A missing/unavailable daemon remains a display concern, not an
            // implicit registration/start or an audited model tool call.
            cache.retry_after = Some(Instant::now() + CACHE_TTL);
            let repository = dispatcher
                .client
                .request(RepositoryResolveParams {
                    path: dispatcher.repository.clone(),
                })
                .await
                .ok()?;
            cache.repository_id = Some(repository.id);
            cache.retry_after = None;
        }
        // Claim freshness before the read so a timeout cannot cause tight
        // retry loops. The key prevents a different displayed thread reusing it.
        cache.last = Some(Cached {
            thread: thread.into(),
            at: Instant::now(),
            value: None,
        });
        let result = dispatcher
            .client
            .request(WorkspaceStatuslineParams {
                repository_id: cache.repository_id.clone().expect("resolved repository"),
                thread_id: thread.into(),
            })
            .await
            .ok()
            .and_then(|result| result.workspace);
        cache.last.as_mut().expect("claimed cache entry").value = result.clone();
        result
    }
}
