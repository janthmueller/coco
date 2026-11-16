use super::*;

#[test]
fn focus_uses_stable_identity_after_reordering_and_relabeling() {
    let mut dialog = Dialog::new([Pick("Sources", "uuid-b"), Pick("Sources", "uuid-b")]);
    let initial = [
        Choice::new("same", None).with_key("uuid-a"),
        Choice::new("same", None).with_key("uuid-b"),
    ];
    let reordered = [
        Choice::new("renamed", None).with_key("uuid-b"),
        Choice::new("same", None).with_key("uuid-a"),
    ];
    {
        let mut adapter = super::super::super::defaults::DefaultsInteraction::new(&mut dialog);
        adapter.begin_step("Sources", true);
        assert_eq!(adapter.select("Sources", &initial).unwrap(), 1);
        assert_eq!(adapter.select("Sources", &reordered).unwrap(), 0);
    }
    assert_eq!(dialog.focus[1].1.as_deref(), Some("uuid-b"));
    dialog.consumed();
}

#[test]
fn missing_identity_is_not_replaced_with_another_same_named_row() {
    let mut dialog = Dialog::new([
        Pick("Sources", "gone"),
        Tab("Other"),
        Pick("Sources", "replacement"),
    ]);
    {
        let mut adapter = super::super::super::defaults::DefaultsInteraction::new(&mut dialog);
        adapter.begin_step("Sources", true);
        adapter
            .select("Sources", &[Choice::new("same", None).with_key("gone")])
            .unwrap();
        adapter
            .select(
                "Other",
                &[Choice::new("Default", None).with_default_marker()],
            )
            .unwrap();
        adapter
            .select(
                "Sources",
                &[Choice::new("same", None)
                    .with_key("replacement")
                    .with_default_marker()],
            )
            .unwrap();
        assert!(!adapter.using_defaults());
    }
    assert_eq!(dialog.focus[2].1, None);
    assert!(
        dialog
            .notices
            .iter()
            .any(|message| message.contains("no longer available"))
    );
    dialog.consumed();
}

#[test]
fn tab_keeps_confirmed_choices_instead_of_overwriting_with_a_product_default() {
    let mut dialog = Dialog::new([Pick("Mode", "Custom"), Tab("Other")]);
    let choices = [
        Choice::new("Default", None).with_default_marker(),
        Choice::new("Custom", None),
    ];
    {
        let mut adapter = super::super::super::defaults::DefaultsInteraction::new(&mut dialog);
        adapter.begin_step("Mode", true);
        assert_eq!(adapter.select("Mode", &choices).unwrap(), 1);
        adapter.select("Other", &choices).unwrap();
        assert_eq!(adapter.select("Mode", &choices).unwrap(), 1);
    }
    dialog.consumed();
}
