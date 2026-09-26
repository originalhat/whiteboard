use wb_core::{
    Actor, Board, Change, Draft, EdgeId, Error, Event, GroupId, ItemId, NodeId, NodeKind, Point,
    replay,
};

fn node(n: u32, label: &str) -> Change {
    Change::NodeAdded {
        id: NodeId(n),
        kind: NodeKind::Service,
        label: label.into(),
        at: Point::new(0.0, n as f64 * 200.0),
        attached_to: None,
    }
}

fn edge(n: u32, from: u32, to: u32) -> Change {
    Change::EdgeAdded {
        id: EdgeId(n),
        from: NodeId(from),
        to: NodeId(to),
        label: None,
    }
}

fn build(changes: Vec<(Actor, Change)>) -> Result<(Board, Vec<Event>), Error> {
    let mut board = Board::default();
    let mut events = Vec::new();
    for (actor, change) in changes {
        events.push(board.commit(Draft::new(actor, change), 0)?);
    }
    Ok((board, events))
}

#[test]
fn an_empty_log_is_an_empty_board_on_the_agents_first_turn() {
    let board = replay(&[]).unwrap();

    assert!(board.is_empty());
    assert_eq!(board.turn, 1);
    assert_eq!(board.holder, Actor::Agent);
}

#[test]
fn replaying_committed_events_rebuilds_the_same_board() {
    let (board, events) = build(vec![
        (Actor::Agent, node(1, "Order Service")),
        (Actor::Human, node(2, "Orders DB")),
        (Actor::Agent, edge(1, 1, 2)),
    ])
    .unwrap();

    assert_eq!(replay(&events).unwrap(), board);
    assert_eq!(board.nodes[&NodeId(2)].by, Actor::Human);
    assert_eq!(board.edges[&EdgeId(1)].from, NodeId(1));
    assert_eq!(board.last_seq, 3);
}

#[test]
fn an_edge_needs_both_endpoints_to_exist() {
    let result = build(vec![
        (Actor::Agent, node(1, "A")),
        (Actor::Agent, edge(1, 1, 9)),
    ]);

    assert_eq!(
        result.unwrap_err(),
        Error::NotFound(ItemId::Node(NodeId(9)))
    );
}

#[test]
fn deleting_a_node_takes_its_edges_and_group_membership_with_it() {
    let (board, _) = build(vec![
        (Actor::Agent, node(1, "A")),
        (Actor::Agent, node(2, "B")),
        (Actor::Agent, node(3, "C")),
        (Actor::Agent, edge(1, 1, 2)),
        (Actor::Agent, edge(2, 2, 3)),
        (
            Actor::Agent,
            Change::GroupAdded {
                id: GroupId(1),
                label: "Core".into(),
                members: vec![NodeId(1), NodeId(2)],
            },
        ),
        (
            Actor::Agent,
            Change::ItemDeleted {
                id: ItemId::Node(NodeId(2)),
            },
        ),
    ])
    .unwrap();

    assert!(board.edges.is_empty());
    assert_eq!(board.groups[&GroupId(1)].members, vec![NodeId(1)]);
}

#[test]
fn deleting_an_item_detaches_notes_about_it() {
    let (board, _) = build(vec![
        (Actor::Agent, node(1, "A")),
        (
            Actor::Agent,
            Change::NodeAdded {
                id: NodeId(2),
                kind: NodeKind::Note,
                label: "why?".into(),
                at: Point::new(0.0, 0.0),
                attached_to: Some(ItemId::Node(NodeId(1))),
            },
        ),
        (
            Actor::Agent,
            Change::ItemDeleted {
                id: ItemId::Node(NodeId(1)),
            },
        ),
    ])
    .unwrap();

    assert_eq!(board.nodes[&NodeId(2)].attached_to, None);
}

#[test]
fn ids_are_never_reused_even_after_a_delete() {
    let result = build(vec![
        (Actor::Agent, node(1, "A")),
        (
            Actor::Agent,
            Change::ItemDeleted {
                id: ItemId::Node(NodeId(1)),
            },
        ),
        (Actor::Agent, node(1, "A again")),
    ]);

    assert_eq!(
        result.unwrap_err(),
        Error::IdReused(ItemId::Node(NodeId(1)))
    );
}

#[test]
fn events_must_arrive_in_sequence() {
    let (_, mut events) = build(vec![
        (Actor::Agent, node(1, "A")),
        (Actor::Agent, node(2, "B")),
    ])
    .unwrap();
    events.swap(0, 1);

    assert_eq!(
        replay(&events).unwrap_err(),
        Error::OutOfSequence {
            expected: 1,
            got: 2
        }
    );
}

#[test]
fn passing_the_turn_hands_it_to_the_other_side_and_starts_a_new_turn() {
    let (board, events) = build(vec![
        (Actor::Agent, node(1, "A")),
        (
            Actor::Agent,
            Change::TurnPassed {
                message: None,
                pointing_at: vec![],
            },
        ),
        (
            Actor::Human,
            Change::TurnPassed {
                message: Some("make it async".into()),
                pointing_at: vec![],
            },
        ),
        (Actor::Agent, node(2, "B")),
    ])
    .unwrap();

    assert_eq!(board.turn, 3);
    assert_eq!(board.holder, Actor::Agent);
    let turns: Vec<u32> = events.iter().map(|e| e.turn).collect();
    assert_eq!(turns, vec![1, 1, 2, 3]);
}

#[test]
fn only_the_side_holding_the_turn_can_pass_it() {
    let result = build(vec![(
        Actor::Human,
        Change::TurnPassed {
            message: None,
            pointing_at: vec![],
        },
    )]);

    assert_eq!(result.unwrap_err(), Error::NotYourTurn(Actor::Human));
}

#[test]
fn either_side_can_edit_whoever_holds_the_turn() {
    let (board, _) = build(vec![(Actor::Human, node(1, "drawn while the agent works"))]).unwrap();

    assert_eq!(board.nodes.len(), 1);
}

#[test]
fn relabeling_changes_the_label_of_nodes_edges_and_groups() {
    let (board, _) = build(vec![
        (Actor::Agent, node(1, "A")),
        (Actor::Agent, node(2, "B")),
        (Actor::Agent, edge(1, 1, 2)),
        (
            Actor::Agent,
            Change::ItemRelabeled {
                id: ItemId::Node(NodeId(1)),
                label: "Checkout".into(),
            },
        ),
        (
            Actor::Agent,
            Change::ItemRelabeled {
                id: ItemId::Edge(EdgeId(1)),
                label: "calls".into(),
            },
        ),
    ])
    .unwrap();

    assert_eq!(board.nodes[&NodeId(1)].label, "Checkout");
    assert_eq!(board.edges[&EdgeId(1)].label.as_deref(), Some("calls"));
}

#[test]
fn a_rejected_event_leaves_the_board_untouched() {
    let (mut board, _) = build(vec![(Actor::Agent, node(1, "A"))]).unwrap();
    let before = board.clone();

    let result = board.commit(Draft::new(Actor::Agent, edge(1, 1, 9)), 0);

    assert!(result.is_err());
    assert_eq!(board, before);
}

#[test]
fn events_serialize_with_a_dotted_kind_and_a_payload() {
    let (_, events) = build(vec![(Actor::Agent, node(1, "Order Service"))]).unwrap();

    let json = serde_json::to_value(&events[0]).unwrap();

    assert_eq!(
        json,
        serde_json::json!({
            "seq": 1,
            "at": 0,
            "actor": "agent",
            "turn": 1,
            "kind": "node.added",
            "payload": {
                "id": "n1",
                "kind": "service",
                "label": "Order Service",
                "at": { "x": 0.0, "y": 200.0 }
            }
        })
    );
    let back: Event = serde_json::from_value(json).unwrap();
    assert_eq!(back, events[0]);
}
