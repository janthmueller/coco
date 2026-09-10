#![cfg(unix)]

use std::fs;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail, ensure};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::net::{TcpListener, TcpStream};
use tokio::process::{Child, Command};
use tokio::sync::oneshot;
use tokio::time::{sleep, timeout};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::server::Callback;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::{Message, http::header::AUTHORIZATION};
use tokio_tungstenite::{WebSocketStream, accept_hdr_async, client_async};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const THREAD_ID: &str = "thread-process-smoke";
const TURN_ID: &str = "turn-process-smoke";
const WAITED_TURN_ID: &str = "turn-process-smoke-waited";
const WORKSPACE_NAME: &str = "feat/process-smoke";
const FORK_SOURCE_THREAD_ID: &str = "thread-fork-source";
const FORK_CHILD_THREAD_ID: &str = "thread-fork-child";
const FORK_SOURCE_WORKSPACE: &str = "feat/fork-source";
const FORK_CHILD_WORKSPACE: &str = "review/fork-child";
const FORK_CHILD_MESSAGE: &str = "Review the inherited work";
const FRESH_JUMP_WORKSPACE: &str = "feat/fresh-jump";
const FRESH_EMPTY_THREAD_ID: &str = "thread-fresh-empty";
const FRESH_ACTIVE_THREAD_ID: &str = "thread-fresh-active";
const FRESH_ACTIVE_TURN_ID: &str = "turn-fresh-active";
const PROFILE_NAME: &str = "process";
const PROFILE_MODEL: &str = "gpt-profile";
const MODEL_OVERRIDE: &str = "gpt-explicit";
const DEFAULT_MODEL: &str = "gpt-default";

#[path = "process_smoke/app_server.rs"]
mod app_server;
#[path = "process_smoke/fork.rs"]
mod fork;
#[path = "process_smoke/fresh_jump.rs"]
mod fresh_jump;
#[path = "process_smoke/hooks.rs"]
mod hooks;
#[path = "process_smoke/lifecycle.rs"]
mod lifecycle;
#[path = "support/mcp_client.rs"]
mod mcp_client;
#[path = "process_smoke/multi_client.rs"]
mod multi_client;
#[path = "process_smoke/retirement.rs"]
mod retirement;
#[path = "process_smoke/signals.rs"]
mod signals;
#[path = "process_smoke/support.rs"]
mod support;
