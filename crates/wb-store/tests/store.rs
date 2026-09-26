use wb_core::{
    Actor, Change, Draft, EdgeId, Error as CoreError, ItemId, NodeId, NodeKind, Point, replay,
};
use wb_store::{Error, Store};

fn node(n: u32, label: &str) -> Draft {
    Draft::new(
        Actor::Agent,
        Change::NodeAdded {
            id: NodeId(n),
            kind: NodeKind::Service,
            label: label.into(),
            at: Point::new(n as f64 * 240.0, 0.0),
            attached_to: None,
        },
    )
}

fn edge(n: u32, from: u32, to: u32) -> Draft {
    Draft::new(
        Actor::Agent,
        Change::EdgeAdded {
            id: EdgeId(n),
            from: NodeId(from),
            to: NodeId(to),
            label: None,
        },
    )
}

#[test]
fn boards_get_short_readable_ids_and_remember_where_they_came_from() {
    let mut store = Store::in_memory().unwrap();

    let first = store.create_board("Checkout", Some("LIN-123"), 1).unwrap();
    let second = store.create_board("Idea", None, 2).unwrap();

    assert_eq!(first.id, "b1");
    assert_eq!(second.id, "b2");
    assert_eq!(store.find_by_source("LIN-123").unwrap(), Some(first));
    assert_eq!(store.find_by_source("LIN-999").unwrap(), None);
}

#[test]
fn appended_events_come_back_in_order_and_load_replays_them() {
    let mut store = Store::in_memory().unwrap();
    let board = store.create_board("Checkout", None, 0).unwrap();

    store.append(&board.id, node(1, "A"), 10).unwrap();
    store.append(&board.id, node(2, "B"), 11).unwrap();
    store.append(&board.id, edge(1, 1, 2), 12).unwrap();

    let events = store.events(&board.id, 0).unwrap();
    assert_eq!(
        events.iter().map(|e| e.seq).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(events[2].at, 12);
    assert_eq!(store.events(&board.id, 2).unwrap().len(), 1);
    assert_eq!(store.load(&board.id).unwrap(), replay(&events).unwrap());
}

#[test]
fn an_invalid_edit_is_rejected_and_nothing_is_stored() {
    let mut store = Store::in_memory().unwrap();
    let board = store.create_board("Checkout", None, 0).unwrap();
    store.append(&board.id, node(1, "A"), 0).unwrap();

    let result = store.append(&board.id, edge(1, 1, 9), 0);

    assert!(matches!(
        result,
        Err(Error::Core(CoreError::NotFound(ItemId::Node(NodeId(9)))))
    ));
    assert_eq!(store.events(&board.id, 0).unwrap().len(), 1);
}

#[test]
fn unknown_boards_are_reported_by_id() {
    let store = Store::in_memory().unwrap();

    assert!(matches!(store.load("b42"), Err(Error::BoardNotFound(id)) if id == "b42"));
}

#[test]
fn snapshots_are_only_a_cache() {
    let mut store = Store::in_memory().unwrap().snapshot_every(2);
    let board = store.create_board("Checkout", None, 0).unwrap();
    for n in 1..=5 {
        store
            .append(&board.id, node(n, &format!("node {n}")), 0)
            .unwrap();
    }
    let with_snapshot = store.load(&board.id).unwrap();

    store.drop_snapshots().unwrap();

    assert_eq!(store.load(&board.id).unwrap(), with_snapshot);
    assert_eq!(with_snapshot.nodes.len(), 5);
}

#[test]
fn a_retried_edit_with_the_same_client_id_is_applied_once() {
    let mut store = Store::in_memory().unwrap();
    let board = store.create_board("Checkout", None, 0).unwrap();
    let mut draft = node(1, "A");
    draft.client_id = Some("ipad-17".into());

    let first = store.append(&board.id, draft.clone(), 0).unwrap();
    let retry = store.append(&board.id, draft, 5).unwrap();

    assert_eq!(first, retry);
    assert_eq!(store.events(&board.id, 0).unwrap().len(), 1);
}

#[test]
fn everything_survives_reopening_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wb.db");
    let id = {
        let mut store = Store::open(&path).unwrap();
        let board = store.create_board("Checkout", Some("LIN-1"), 0).unwrap();
        store.append(&board.id, node(1, "A"), 0).unwrap();
        store.set_active(&board.id).unwrap();
        board.id
    };

    let store = Store::open(&path).unwrap();

    assert_eq!(store.active().unwrap(), Some(id.clone()));
    assert_eq!(store.load(&id).unwrap().nodes.len(), 1);
}

#[test]
fn listing_boards_includes_how_many_events_each_has() {
    let mut store = Store::in_memory().unwrap();
    let a = store.create_board("A", None, 0).unwrap();
    store.create_board("B", Some("LIN-2"), 0).unwrap();
    store.append(&a.id, node(1, "x"), 0).unwrap();

    let boards = store.boards().unwrap();

    let summary: Vec<(String, u64)> = boards
        .iter()
        .map(|b| (b.meta.id.clone(), b.events))
        .collect();
    assert_eq!(summary, vec![("b1".into(), 1), ("b2".into(), 0)]);
}

#[test]
fn export_then_import_gives_an_identical_board_under_a_new_id() {
    let mut store = Store::in_memory().unwrap();
    let board = store.create_board("Checkout", Some("LIN-123"), 7).unwrap();
    store.append(&board.id, node(1, "A"), 0).unwrap();
    store.append(&board.id, node(2, "B"), 0).unwrap();
    store.append(&board.id, edge(1, 1, 2), 0).unwrap();

    let exported = store.export(&board.id).unwrap();
    let imported = store.import(&exported).unwrap();

    assert_eq!(exported.lines().count(), 4);
    assert!(
        exported
            .lines()
            .next()
            .unwrap()
            .contains("\"format\":\"wb/0.1\"")
    );
    assert_ne!(imported.id, board.id);
    assert_eq!(imported.title, "Checkout");
    assert_eq!(imported.source.as_deref(), Some("LIN-123"));
    assert_eq!(
        store.events(&imported.id, 0).unwrap(),
        store.events(&board.id, 0).unwrap()
    );
    assert_eq!(
        store.load(&imported.id).unwrap(),
        store.load(&board.id).unwrap()
    );
}

#[test]
fn import_refuses_other_format_versions() {
    let mut store = Store::in_memory().unwrap();
    let file = r#"{"format":"wb/9.0","board":{"id":"b1","title":"x","created_at":0}}"#;

    assert!(matches!(store.import(file), Err(Error::UnsupportedFormat(v)) if v == "wb/9.0"));
    assert!(store.boards().unwrap().is_empty());
}

#[test]
fn import_refuses_a_log_that_does_not_replay_and_stores_nothing() {
    let mut store = Store::in_memory().unwrap();
    let board = store.create_board("Checkout", None, 0).unwrap();
    store.append(&board.id, node(1, "A"), 0).unwrap();
    store.append(&board.id, node(2, "B"), 0).unwrap();
    let exported = store.export(&board.id).unwrap();
    let broken: String = exported
        .lines()
        .enumerate()
        .filter(|(i, _)| *i != 1)
        .map(|(_, l)| format!("{l}\n"))
        .collect();

    let result = store.import(&broken);

    assert!(
        matches!(result, Err(Error::BadExport { line: 2, .. })),
        "{result:?}"
    );
    assert_eq!(store.boards().unwrap().len(), 1);
}
