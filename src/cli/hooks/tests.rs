use super::*;
#[test]
fn hook_commands_offer_consistent_collection_aliases() {
    use clap::CommandFactory;

    let mut command = crate::cli::Cli::command();
    let hook = command.find_subcommand_mut("hook").unwrap();
    assert!(
        hook.find_subcommand("list")
            .unwrap()
            .get_all_aliases()
            .any(|alias| alias == "ls")
    );
    assert!(
        hook.find_subcommand("history")
            .unwrap()
            .get_all_aliases()
            .any(|alias| alias == "deliveries")
    );
    assert!(hook.find_subcommand("validate").is_some());
    assert!(hook.find_subcommand("reload").is_some());
}

#[test]
fn delivery_state_presentation_is_unambiguous() {
    assert_eq!(
        delivery_presentation(HookDeliveryState::Succeeded).2,
        "Succeeded"
    );
    assert_eq!(delivery_presentation(HookDeliveryState::Failed).0, "✗");
    assert_eq!(HookEventKind::SignalEmitted.as_str(), "signal.emitted");
}
