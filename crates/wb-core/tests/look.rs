use wb_core::{
    Actor, AnnotationId, Board, Change, Draft, EdgeId, GroupId, ItemId, NodeId, NodeKind, Point,
    Relation, StrokePoint, Target, look,
};

fn commit(board: &mut Board, actor: Actor, change: Change) {
    board.commit(Draft::new(actor, change), 0).unwrap();
}

fn node(board: &mut Board, actor: Actor, n: u32, kind: NodeKind, label: &str) {
    commit(
        board,
        actor,
        Change::NodeAdded {
            id: NodeId(n),
            kind,
            label: label.into(),
            at: Point::new(n as f64 * 240.0, 0.0),
            attached_to: None,
        },
    );
}

fn sample() -> Board {
    let mut board = Board::default();
    node(
        &mut board,
        Actor::Agent,
        1,
        NodeKind::Service,
        "Order Service",
    );
    node(&mut board, Actor::Human, 2, NodeKind::Db, "Orders, primary");
    node(&mut board, Actor::Agent, 3, NodeKind::Queue, "Order Events");
    commit(
        &mut board,
        Actor::Agent,
        Change::EdgeAdded {
            id: EdgeId(1),
            from: NodeId(1),
            to: NodeId(2),
            label: Some("writes".into()),
        },
    );
    commit(
        &mut board,
        Actor::Agent,
        Change::EdgeAdded {
            id: EdgeId(2),
            from: NodeId(1),
            to: NodeId(3),
            label: None,
        },
    );
    commit(
        &mut board,
        Actor::Agent,
        Change::GroupAdded {
            id: GroupId(1),
            label: "Orders".into(),
            members: vec![NodeId(1), NodeId(2)],
        },
    );
    commit(
        &mut board,
        Actor::Agent,
        Change::NodeAdded {
            id: NodeId(4),
            kind: NodeKind::Note,
            label: "Sync or \"async\"?".into(),
            at: Point::new(0.0, 160.0),
            attached_to: Some(ItemId::Node(NodeId(1))),
        },
    );
    commit(
        &mut board,
        Actor::Human,
        Change::AnnotationAdded {
            id: AnnotationId(1),
            stroke: vec![StrokePoint {
                x: 0.0,
                y: 0.0,
                t: 0.0,
                pressure: None,
            }],
            zoom: 1.0,
            targets: vec![Target {
                relation: Relation::Encloses,
                items: vec![ItemId::Node(NodeId(1)), ItemId::Node(NodeId(3))],
            }],
            text: Some("this needs to be async".into()),
        },
    );
    board
}

#[test]
fn look_lists_each_kind_of_item_compactly_with_notes_and_marks_resolved() {
    let expected = "\
nodes[3]{id,kind,label,by}:
  n1,service,Order Service,agent
  n2,db,\"Orders, primary\",human
  n3,queue,Order Events,agent
edges[2]{id,from,to,label}:
  e1,n1,n2,writes
  e2,n1,n3,\"\"
groups[1]{id,label,members}:
  g1,Orders,n1 n2
notes[1]{id,on,text,by}:
  n4,n1,\"Sync or \\\"async\\\"?\",agent
annotations[1]:
  a1: human circled n1 \"Order Service\" and n3 \"Order Events\": \"this needs to be async\"
";

    assert_eq!(look::board(&sample()), expected);
}

#[test]
fn look_says_so_when_the_board_is_empty() {
    assert_eq!(look::board(&Board::default()), "board is empty\n");
}

#[test]
fn marks_on_deleted_items_still_read_sensibly() {
    let mut board = sample();
    commit(
        &mut board,
        Actor::Agent,
        Change::ItemDeleted {
            id: ItemId::Node(NodeId(3)),
        },
    );

    let text = look::board(&board);

    assert!(text.contains("human circled n1 \"Order Service\" and n3 (deleted)"));
}

#[test]
fn counts_summarise_the_board_in_one_line() {
    assert_eq!(
        look::counts(&sample()),
        "3 nodes, 2 edges, 1 group, 1 note, 1 annotation"
    );
}

#[test]
fn describe_turns_each_change_into_a_short_phrase() {
    let cases = [
        (
            Change::NodeAdded {
                id: NodeId(1),
                kind: NodeKind::Service,
                label: "Order Service".into(),
                at: Point::new(0.0, 0.0),
                attached_to: None,
            },
            "added service n1 \"Order Service\"",
        ),
        (
            Change::NodeAdded {
                id: NodeId(4),
                kind: NodeKind::Note,
                label: "why?".into(),
                at: Point::new(0.0, 0.0),
                attached_to: Some(ItemId::Node(NodeId(1))),
            },
            "added note n4 \"why?\" on n1",
        ),
        (
            Change::EdgeAdded {
                id: EdgeId(1),
                from: NodeId(1),
                to: NodeId(2),
                label: Some("writes".into()),
            },
            "connected e1 n1 -> n2 \"writes\"",
        ),
        (
            Change::GroupAdded {
                id: GroupId(1),
                label: "Orders".into(),
                members: vec![NodeId(1), NodeId(2)],
            },
            "grouped g1 \"Orders\": n1 n2",
        ),
        (
            Change::NodeMoved {
                id: NodeId(1),
                to: Point::new(240.0, 0.0),
            },
            "moved n1",
        ),
        (
            Change::ItemRelabeled {
                id: ItemId::Node(NodeId(1)),
                label: "Checkout".into(),
            },
            "relabeled n1 \"Checkout\"",
        ),
        (
            Change::ItemDeleted {
                id: ItemId::Edge(EdgeId(2)),
            },
            "deleted e2",
        ),
        (
            Change::MessageSent {
                text: "Here's my first take".into(),
            },
            "said \"Here's my first take\"",
        ),
        (
            Change::TurnPassed {
                message: Some("make it async".into()),
                pointing_at: vec![],
            },
            "passed the turn: \"make it async\"",
        ),
        (
            Change::TurnPassed {
                message: None,
                pointing_at: vec![],
            },
            "passed the turn",
        ),
        (
            Change::TurnPassed {
                message: Some("is this idempotent?".into()),
                pointing_at: vec![ItemId::Node(NodeId(5))],
            },
            "passed the turn: \"is this idempotent?\" (pointing at n5)",
        ),
        (
            Change::AnnotationAdded {
                id: AnnotationId(1),
                stroke: vec![],
                zoom: 1.0,
                targets: vec![Target {
                    relation: Relation::Encloses,
                    items: vec![ItemId::Node(NodeId(5))],
                }],
                text: None,
            },
            "drew a1 circling n5",
        ),
        (
            Change::AnnotationAdded {
                id: AnnotationId(2),
                stroke: vec![],
                zoom: 1.0,
                targets: vec![],
                text: None,
            },
            "drew a2",
        ),
        (
            Change::ItemRelabeled {
                id: ItemId::Annotation(AnnotationId(1)),
                label: "retries?".into(),
            },
            "commented on a1: \"retries?\"",
        ),
    ];

    for (change, expected) in cases {
        assert_eq!(look::describe(&change), expected);
    }
}
