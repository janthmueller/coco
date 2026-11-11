use super::*;

#[tokio::test]
async fn recent_picker_enriches_only_the_selected_scope_and_revalidates_exact_id() {
    for global in [false, true] {
        let mut interaction = Select::default();
        let (result, calls) = exercise_selection(
            Options {
                global,
                ..Default::default()
            },
            &mut interaction,
        )
        .await;
        result.unwrap();
        assert_eq!(interaction.choices[0].label, "feat/10");
        assert!(interaction.choices[0].trailing_detail.is_some());
        assert!(interaction.choices[1].trailing_detail.is_none());
        let list = &calls[calls.len() - 2];
        let get = calls.last().unwrap();
        assert_eq!(list.0, "workspace.path.list");
        assert_eq!(list.1["includeRecency"], true);
        assert_eq!(get.1["workspace"], "repo-feat/10");
        assert_eq!(list.1["scope"], get.1["scope"]);
        assert_eq!(
            get.1["scope"]["kind"],
            if global {
                "allRepositories"
            } else {
                "repository"
            }
        );
    }
}

#[tokio::test]
async fn name_mode_requests_only_saved_metadata_and_legacy_daemon_falls_back_once() {
    for (sort, legacy) in [(WorkspaceSort::Name, false), (WorkspaceSort::Recent, true)] {
        let mut interaction = Select::default();
        let (result, calls) = exercise_selection(
            Options {
                global: true,
                sort,
                legacy_recency: legacy,
                ..Default::default()
            },
            &mut interaction,
        )
        .await;
        result.unwrap();
        assert_eq!(interaction.choices[0].label, "feat/2");
        assert!(
            interaction
                .choices
                .iter()
                .all(|choice| choice.trailing_detail.is_none())
        );
        let lists = calls
            .iter()
            .filter(|(method, _)| method == "workspace.path.list")
            .collect::<Vec<_>>();
        assert_eq!(lists.len(), if legacy { 2 } else { 1 });
        assert!(lists.last().unwrap().1.get("includeRecency").is_none());
        assert!(
            lists
                .iter()
                .all(|(_, params)| params["scope"] == json!({"kind":"allRepositories"}))
        );
        assert_eq!(calls.last().unwrap().1["workspace"], "repo-feat/2");
    }
}

#[tokio::test]
async fn explicit_target_never_lists_or_enriches_even_with_default_recent_sort() {
    for global in [false, true] {
        let mut interaction = Select {
            noninteractive: true,
            ..Default::default()
        };
        let (result, calls) = exercise_selection(
            Options {
                global,
                workspace: Some("feat/2".to_owned()),
                ..Default::default()
            },
            &mut interaction,
        )
        .await;
        result.unwrap();
        assert!(interaction.choices.is_empty());
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "workspace.path.get");
        assert_eq!(calls[0].1["workspace"], "feat/2");
        assert!(calls[0].1.get("includeRecency").is_none());
    }
}
