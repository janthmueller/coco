use anyhow::Result;
use clap::Subcommand;

use super::output::{print_json, safe_line, versioned, versioned_array};
use super::style::{Palette, Tone};
use crate::domain::hooks::{
    GuardSummary, HookDeliveryState, HookDeliverySummary, HookEventKind, HookRegistrySummary,
    HookSummary,
};
use crate::hooks::HookRegistry;
use crate::paths::CocoPaths;
use crate::protocol::{HookDeliveryListParams, HookListParams, HookReloadParams};
use crate::rpc::RpcClient;

#[derive(Debug, Subcommand)]
pub(super) enum HookCommand {
    /// List the reactions and guards active in the running daemon.
    #[command(visible_alias = "ls")]
    List {
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Validate the hook and guard configuration without executing it.
    Validate {
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Atomically reload hooks and guards in the running daemon.
    Reload {
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
    /// Show recent post-event delivery outcomes and pending retries.
    #[command(visible_alias = "deliveries")]
    History {
        /// Maximum number of newest deliveries to show (1–100).
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        /// Emit stable, machine-readable JSON.
        #[arg(long)]
        json: bool,
    },
}

pub(super) async fn run(command: HookCommand, paths: &CocoPaths) -> Result<()> {
    match command {
        HookCommand::List { json } => {
            let client = RpcClient::new(paths.socket_path.clone());
            let registry = client.request(HookListParams {}).await?;
            if json {
                print_json(versioned(serde_json::to_value(registry)?))
            } else {
                print_registry(&registry);
                Ok(())
            }
        }
        HookCommand::Validate { json } => {
            let registry = HookRegistry::validate(&paths.hooks_path)?;
            if json {
                print_json(versioned(serde_json::to_value(registry)?))
            } else {
                print_config_result("Valid configuration", &registry);
                Ok(())
            }
        }
        HookCommand::Reload { json } => {
            let client = RpcClient::new(paths.socket_path.clone());
            let registry = client.request(HookReloadParams {}).await?;
            if json {
                print_json(versioned(serde_json::to_value(registry)?))
            } else {
                print_config_result("Reloaded", &registry);
                Ok(())
            }
        }
        HookCommand::History { limit, json } => {
            let client = RpcClient::new(paths.socket_path.clone());
            let deliveries = client.request(HookDeliveryListParams { limit }).await?;
            if json {
                print_json(versioned_array(
                    "deliveries",
                    serde_json::to_value(deliveries)?,
                ))
            } else {
                print_history(&deliveries);
                Ok(())
            }
        }
    }
}

fn print_registry(registry: &HookRegistrySummary) {
    if registry.hooks.is_empty() && registry.guards.is_empty() {
        println!(
            "{}",
            Palette::stdout().paint(Tone::Dim, "No hooks or guards configured.")
        );
        return;
    }
    print_hooks(&registry.hooks);
    print_guards(&registry.guards);
}

fn print_hooks(hooks: &[HookSummary]) {
    let palette = Palette::stdout();
    for hook in hooks {
        let id = palette.paint(Tone::CyanBold, safe_line(&hook.id));
        let event = palette.paint(Tone::Dim, hook.event.as_str());
        match hook.signal.as_deref() {
            Some(filter) => println!(
                "{id}  {event}  {}",
                palette.paint(Tone::Primary, safe_line(filter))
            ),
            None if hook.event == HookEventKind::SignalEmitted => println!(
                "{id}  {event}  {}",
                palette.paint(Tone::Primary, "all signals")
            ),
            None => println!("{id}  {event}"),
        }
        println!(
            "  {}",
            palette.paint(
                Tone::Dim,
                format!(
                    "{}s timeout · {} attempt{}",
                    hook.timeout_seconds,
                    hook.max_attempts,
                    if hook.max_attempts == 1 { "" } else { "s" }
                )
            )
        );
    }
}

fn print_guards(guards: &[GuardSummary]) {
    let palette = Palette::stdout();
    for guard in guards {
        println!(
            "{}  {}  {}",
            palette.paint(Tone::CyanBold, safe_line(&guard.id)),
            palette.paint(Tone::Dim, guard.action.as_str()),
            palette.paint(Tone::Primary, "before")
        );
        println!(
            "  {}",
            palette.paint(
                Tone::Dim,
                format!(
                    "{}s timeout · {} on error",
                    guard.timeout_seconds,
                    guard.on_error.as_str()
                )
            )
        );
    }
}

fn print_config_result(label: &str, registry: &HookRegistrySummary) {
    let palette = Palette::stdout();
    println!(
        "{} {} {}",
        palette.paint(Tone::GreenBold, "✓"),
        label,
        palette.paint(
            Tone::Dim,
            format!(
                "· {} hook{} · {} guard{}",
                registry.hooks.len(),
                if registry.hooks.len() == 1 { "" } else { "s" },
                registry.guards.len(),
                if registry.guards.len() == 1 { "" } else { "s" }
            )
        )
    );
}

fn print_history(deliveries: &[HookDeliverySummary]) {
    let palette = Palette::stdout();
    if deliveries.is_empty() {
        println!("{}", palette.paint(Tone::Dim, "No hook deliveries."));
        return;
    }
    for delivery in deliveries {
        let (marker, tone, state) = delivery_presentation(delivery.state);
        println!(
            "{} {}  {} {} {}  {} attempt{}",
            palette.paint(tone, marker),
            palette.paint(Tone::Bold, safe_line(&delivery.hook_id)),
            palette.paint(Tone::Dim, delivery.event.as_str()),
            palette.paint(Tone::Dim, "·"),
            palette.paint(tone, state),
            delivery.attempts,
            if delivery.attempts == 1 { "" } else { "s" },
        );
        if let Some(error) = &delivery.last_error {
            println!("  {}", palette.paint(Tone::Dim, safe_line(error)));
        }
    }
}

const fn delivery_presentation(state: HookDeliveryState) -> (&'static str, Tone, &'static str) {
    match state {
        HookDeliveryState::Pending => ("●", Tone::Cyan, "Pending"),
        HookDeliveryState::Running => ("●", Tone::CyanBold, "Running"),
        HookDeliveryState::Succeeded => ("✓", Tone::Green, "Succeeded"),
        HookDeliveryState::Failed => ("✗", Tone::Red, "Failed"),
        HookDeliveryState::Cancelled => ("○", Tone::Dim, "Cancelled"),
    }
}

#[cfg(test)]
mod tests;
