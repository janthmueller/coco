use crate::protocol::{WorkspaceCostEstimate, WorkspaceUsageItem};

use super::super::style::{Palette, Tone};

pub(super) fn render_workspace_usage_details(
    item: &WorkspaceUsageItem,
    palette: Palette,
) -> String {
    let mut output = String::new();
    if let Some(tokens) = &item.tokens {
        output.push_str(&format!(
            "  Tokens   {} total\n",
            format_integer(tokens.checkpoint.total.total_tokens)
        ));
        output.push_str(&format!(
            "  Input    {} · {} cached\n",
            format_integer(tokens.checkpoint.total.input_tokens),
            format_integer(tokens.checkpoint.total.cached_input_tokens),
        ));
        output.push_str(&format!(
            "  Output   {} · {} reasoning\n",
            format_integer(tokens.checkpoint.total.output_tokens),
            format_integer(tokens.checkpoint.total.reasoning_output_tokens),
        ));
        output.push_str(&format!(
            "  Context  {}\n",
            context_detail(
                tokens.checkpoint.last.total_tokens,
                tokens.checkpoint.model_context_window
            )
        ));
        if !tokens.is_fresh {
            output.push_str(&format!(
                "  {}\n",
                palette.paint(Tone::Dim, "Last seen before this CoCo run")
            ));
        }
    } else {
        output.push_str(&format!(
            "  {}\n",
            palette.paint(Tone::Dim, "No token usage observed yet")
        ));
    }
    output.push_str(&format!(
        "  Cost     {}\n",
        cost_detail(&item.cost, palette)
    ));
    output
}

pub(super) fn usage_cells(item: Option<&WorkspaceUsageItem>) -> (String, String, String) {
    let Some(item) = item else {
        return ("—".to_owned(), "—".to_owned(), "—".to_owned());
    };
    let Some(tokens) = &item.tokens else {
        return ("—".to_owned(), "—".to_owned(), cost_cell(&item.cost));
    };
    let mut total = format_compact_tokens(tokens.checkpoint.total.total_tokens);
    if !tokens.is_fresh {
        total.push_str(" (last seen)");
    }
    let context = context_cell(
        tokens.checkpoint.last.total_tokens,
        tokens.checkpoint.model_context_window,
    );
    (total, context, cost_cell(&item.cost))
}

fn context_cell(current: u64, window: Option<u64>) -> String {
    window.map_or_else(
        || "—".to_owned(),
        |window| format!("{:.1}%", current as f64 * 100.0 / window as f64),
    )
}

fn context_detail(current: u64, window: Option<u64>) -> String {
    window.map_or_else(
        || "—".to_owned(),
        |window| {
            format!(
                "{} / {} · {:.1}%",
                format_integer(current),
                format_integer(window),
                current as f64 * 100.0 / window as f64
            )
        },
    )
}

fn cost_cell(cost: &WorkspaceCostEstimate) -> String {
    match cost {
        WorkspaceCostEstimate::Available {
            estimated_usage_usd_micros: Some(value),
            ..
        } => format!("${}", format_micros(*value)),
        WorkspaceCostEstimate::Available {
            estimated_usage_credits_micros,
            ..
        } => format!("{} credits", format_micros(*estimated_usage_credits_micros)),
        WorkspaceCostEstimate::Unavailable { .. } => "—".to_owned(),
    }
}

fn cost_detail(cost: &WorkspaceCostEstimate, palette: Palette) -> String {
    match cost {
        WorkspaceCostEstimate::Available { .. } => {
            format!(
                "{} {}",
                cost_cell(cost),
                palette.paint(Tone::Dim, "estimate")
            )
        }
        WorkspaceCostEstimate::Unavailable { reason, .. } => {
            let detail = match reason {
                crate::protocol::WorkspaceCostUnavailableReason::NoThread => "— no thread yet",
                crate::protocol::WorkspaceCostUnavailableReason::NotReported => {
                    "— not reported by Codex"
                }
                crate::protocol::WorkspaceCostUnavailableReason::ReadFailed => "— unavailable",
            };
            palette.paint(Tone::Dim, detail)
        }
    }
}

fn format_compact_tokens(tokens: u64) -> String {
    const THOUSAND: f64 = 1_000.0;
    const MILLION: f64 = 1_000_000.0;
    const BILLION: f64 = 1_000_000_000.0;
    let tokens_float = tokens as f64;
    if tokens_float >= BILLION {
        compact_decimal(tokens_float / BILLION, "b")
    } else if tokens_float >= MILLION {
        compact_decimal(tokens_float / MILLION, "m")
    } else if tokens_float >= THOUSAND {
        compact_decimal(tokens_float / THOUSAND, "k")
    } else {
        tokens.to_string()
    }
}

fn compact_decimal(value: f64, suffix: &str) -> String {
    let value = format!("{value:.1}");
    format!("{}{suffix}", value.trim_end_matches(".0"))
}

fn format_integer(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);
    let first = digits.len() % 3;
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && index % 3 == first {
            output.push(',');
        }
        output.push(character);
    }
    output
}

fn format_micros(value: u64) -> String {
    let whole = value / 1_000_000;
    let fraction = value % 1_000_000;
    if fraction == 0 {
        return whole.to_string();
    }
    let fraction = format!("{fraction:06}");
    format!("{whole}.{}", fraction.trim_end_matches('0'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_counts_and_context_are_scan_friendly() {
        assert_eq!(format_compact_tokens(999), "999");
        assert_eq!(format_compact_tokens(12_300), "12.3k");
        assert_eq!(format_compact_tokens(2_000_000), "2m");
        assert_eq!(context_cell(84_000, Some(200_000)), "42.0%");
        assert_eq!(context_cell(84_000, None), "—");
        assert_eq!(format_integer(12_345_678), "12,345,678");
    }

    #[test]
    fn native_cost_prefers_currency_and_keeps_credits_as_fallback() {
        let usd = WorkspaceCostEstimate::Available {
            estimated_usage_credits_micros: 1_250_000,
            estimated_usage_usd_micros: Some(420_000),
            groups: Vec::new(),
            observed_at_ms: 1,
        };
        assert_eq!(cost_cell(&usd), "$0.42");
        let credits = WorkspaceCostEstimate::Available {
            estimated_usage_credits_micros: 1_250_000,
            estimated_usage_usd_micros: None,
            groups: Vec::new(),
            observed_at_ms: 1,
        };
        assert_eq!(cost_cell(&credits), "1.25 credits");
    }
}
