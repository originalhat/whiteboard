use wb_core::Intent::{self, *};
use wb_core::{
    Actor, Board, Change, Draft, EdgeId, Error, ItemId, NodeId, NodeKind, Outcome, Placement,
    Point, Unchanged,
};

fn apply(board: &mut Board, intent: Intent) -> Outcome {
    let outcome = board.plan(&intent).unwrap();
    if let Outcome::Changed(change) = &outcome {
        board
            .commit(Draft::new(Actor::Agent, change.clone()), 0)
            .unwrap();
    }
    outcome
}

fn add(board: &mut Board, label: &str, placement: Placement) -> NodeId {
    apply(
        board,
        Add {
            kind: NodeKind::Service,
            label: label.into(),
            placement,
        },
    );
    board.nodes.values().find(|n| n.label == label).unwrap().id
}

fn at(board: &Board, id: NodeId) -> Point {
    board.nodes[&id].at
}

fn n(id: NodeId) -> ItemId {
    id.into()
}

fn assert_no_overlaps(board: &Board) {
    let nodes: Vec<_> = board.nodes.values().collect();
    for (i, a) in nodes.iter().enumerate() {
        for b in &nodes[i + 1..] {
            assert!(
                !wb_core::layout::overlaps(a.at, b.at),
                "{} at {:?} overlaps {} at {:?}",
                a.id,
                a.at,
                b.id,
                b.at
            );
        }
    }
}

#[test]
fn the_first_node_lands_at_the_origin() {
    let mut board = Board::default();

    let id = add(&mut board, "Order Service", Placement::Auto);

    assert_eq!(at(&board, id), Point::new(0.0, 0.0));
}

#[test]
fn auto_placed_nodes_never_overlap() {
    let mut board = Board::default();

    for i in 0..12 {
        add(&mut board, &format!("node {i}"), Placement::Auto);
    }

    assert_eq!(board.nodes.len(), 12);
    assert_no_overlaps(&board);
}

#[test]
fn right_of_keeps_the_row_and_skips_past_taken_spots() {
    let mut board = Board::default();
    let a = add(&mut board, "A", Placement::Auto);
    let b = add(&mut board, "B", Placement::RightOf(n(a)));
    let c = add(&mut board, "C", Placement::RightOf(n(a)));

    assert_eq!(at(&board, b).y, at(&board, a).y);
    assert!(at(&board, b).x > at(&board, a).x);
    assert_eq!(at(&board, c).y, at(&board, a).y);
    assert!(at(&board, c).x > at(&board, b).x);
    assert_no_overlaps(&board);
}

#[test]
fn below_keeps_the_column() {
    let mut board = Board::default();
    let a = add(&mut board, "A", Placement::Auto);
    let b = add(&mut board, "B", Placement::Below(n(a)));

    assert_eq!(at(&board, b).x, at(&board, a).x);
    assert!(at(&board, b).y > at(&board, a).y);
}

#[test]
fn left_of_and_above_go_the_other_way() {
    let mut board = Board::default();
    let a = add(&mut board, "A", Placement::Auto);
    let b = add(&mut board, "B", Placement::LeftOf(n(a)));
    let c = add(&mut board, "C", Placement::Above(n(a)));

    assert!(at(&board, b).x < at(&board, a).x);
    assert!(at(&board, c).y < at(&board, a).y);
}

#[test]
fn near_finds_a_free_spot_next_to_the_anchor() {
    let mut board = Board::default();
    let a = add(&mut board, "A", Placement::Auto);
    for i in 0..5 {
        add(&mut board, &format!("near {i}"), Placement::Near(n(a)));
    }

    assert_no_overlaps(&board);
    for node in board.nodes.values() {
        let d = at(&board, a);
        assert!((node.at.x - d.x).abs() <= 240.0 && (node.at.y - d.y).abs() <= 160.0);
    }
}

#[test]
fn placing_relative_to_a_missing_item_is_an_error() {
    let board = Board::default();

    let result = board.plan(&Add {
        kind: NodeKind::Db,
        label: "Orders".into(),
        placement: Placement::RightOf(ItemId::Node(NodeId(7))),
    });

    assert_eq!(
        result.unwrap_err(),
        Error::NotFound(ItemId::Node(NodeId(7)))
    );
}

#[test]
fn a_note_on_an_item_sits_beside_it_and_remembers_what_it_is_about() {
    let mut board = Board::default();
    let a = add(&mut board, "A", Placement::Auto);

    apply(
        &mut board,
        Note {
            text: "sync or async?".into(),
            on: Some(n(a)),
        },
    );

    let note = board
        .nodes
        .values()
        .find(|n| n.kind == NodeKind::Note)
        .unwrap();
    assert_eq!(note.attached_to, Some(n(a)));
    assert!(!wb_core::layout::overlaps(note.at, at(&board, a)));
}

#[test]
fn adding_the_same_node_twice_is_a_no_op_that_names_the_existing_one() {
    let mut board = Board::default();
    let a = add(&mut board, "Orders DB", Placement::Auto);

    let again = board
        .plan(&Add {
            kind: NodeKind::Service,
            label: "Orders DB".into(),
            placement: Placement::Auto,
        })
        .unwrap();

    assert_eq!(again, Outcome::Unchanged(Unchanged::Exists(n(a))));
}

#[test]
fn connecting_the_same_pair_twice_is_a_no_op() {
    let mut board = Board::default();
    let a = add(&mut board, "A", Placement::Auto);
    let b = add(&mut board, "B", Placement::Auto);
    let connect = Connect {
        from: n(a),
        to: n(b),
        label: Some("writes".into()),
    };
    apply(&mut board, connect.clone());

    assert_eq!(
        board.plan(&connect).unwrap(),
        Outcome::Unchanged(Unchanged::Exists(ItemId::Edge(EdgeId(1))))
    );
}

#[test]
fn only_nodes_can_be_connected() {
    let mut board = Board::default();
    let a = add(&mut board, "A", Placement::Auto);
    let b = add(&mut board, "B", Placement::Auto);
    apply(
        &mut board,
        Connect {
            from: n(a),
            to: n(b),
            label: None,
        },
    );

    let result = board.plan(&Connect {
        from: ItemId::Edge(EdgeId(1)),
        to: n(a),
        label: None,
    });

    assert_eq!(
        result.unwrap_err(),
        Error::NotANode(ItemId::Edge(EdgeId(1)))
    );
}

#[test]
fn a_group_label_can_only_be_used_once() {
    let mut board = Board::default();
    let a = add(&mut board, "A", Placement::Auto);
    let b = add(&mut board, "B", Placement::Auto);
    apply(
        &mut board,
        Group {
            label: "Payments".into(),
            members: vec![n(a)],
        },
    );

    let same = board
        .plan(&Group {
            label: "Payments".into(),
            members: vec![n(a)],
        })
        .unwrap();
    let different = board.plan(&Group {
        label: "Payments".into(),
        members: vec![n(a), n(b)],
    });

    assert!(matches!(same, Outcome::Unchanged(Unchanged::Exists(_))));
    assert!(matches!(different, Err(Error::LabelTaken(_))));
}

#[test]
fn moving_a_node_places_it_without_overlapping_and_moving_it_again_is_a_no_op() {
    let mut board = Board::default();
    let a = add(&mut board, "A", Placement::Auto);
    let b = add(&mut board, "B", Placement::Auto);
    let c = add(&mut board, "C", Placement::Auto);

    let first = apply(
        &mut board,
        Move {
            id: n(c),
            placement: Placement::Below(n(a)),
        },
    );
    let second = board
        .plan(&Move {
            id: n(c),
            placement: Placement::Below(n(a)),
        })
        .unwrap();

    assert!(matches!(first, Outcome::Changed(Change::NodeMoved { .. })));
    assert_eq!(at(&board, c).x, at(&board, a).x);
    assert_eq!(second, Outcome::Unchanged(Unchanged::NoChange(n(c))));
    assert_no_overlaps(&board);
    let _ = b;
}

#[test]
fn relabeling_to_the_same_label_is_a_no_op() {
    let mut board = Board::default();
    let a = add(&mut board, "A", Placement::Auto);

    let outcome = board
        .plan(&Relabel {
            id: n(a),
            label: "A".into(),
        })
        .unwrap();

    assert_eq!(outcome, Outcome::Unchanged(Unchanged::NoChange(n(a))));
}

#[test]
fn deleting_twice_says_it_is_already_gone_but_unknown_ids_are_errors() {
    let mut board = Board::default();
    let a = add(&mut board, "A", Placement::Auto);
    apply(&mut board, Delete { id: n(a) });

    assert_eq!(
        board.plan(&Delete { id: n(a) }).unwrap(),
        Outcome::Unchanged(Unchanged::AlreadyDeleted(n(a)))
    );
    assert_eq!(
        board
            .plan(&Delete {
                id: ItemId::Node(NodeId(99))
            })
            .unwrap_err(),
        Error::NotFound(ItemId::Node(NodeId(99)))
    );
}
