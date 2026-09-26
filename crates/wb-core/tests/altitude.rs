use wb_core::Intent::{Add, Level};
use wb_core::{
    Actor, Board, Change, Draft, EdgeId, Error, Intent, ItemId, NodeId, NodeKind, Outcome,
    Placement, look, view,
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

fn add(board: &mut Board, label: &str, altitude: Option<u8>, parent: Option<u32>) -> NodeId {
    apply(
        board,
        Add {
            kind: NodeKind::Service,
            label: label.into(),
            placement: Placement::Auto,
            altitude,
            parent: parent.map(|p| ItemId::Node(NodeId(p))),
        },
    );
    board.nodes.values().find(|n| n.label == label).unwrap().id
}

fn connect(board: &mut Board, from: NodeId, to: NodeId, label: &str) {
    apply(
        board,
        Intent::Connect {
            from: from.into(),
            to: to.into(),
            label: Some(label.into()),
        },
    );
}

/// Checkout (n1) and Billing (n2) at the top. Inside Checkout: Cart (n3) and CreatePayment (n4).
/// Inside CreatePayment: Retry (n5), at code level. Edges: n3 -> n4, n4 -> n2, n5 -> n2.
fn system() -> Board {
    let mut board = Board::default();
    let checkout = add(&mut board, "Checkout", Some(1), None);
    let billing = add(&mut board, "Billing", Some(1), None);
    let cart = add(&mut board, "Cart", None, Some(1));
    let create = add(&mut board, "CreatePayment", None, Some(1));
    let retry = add(&mut board, "Retry", Some(4), Some(4));
    connect(&mut board, cart, create, "adds");
    connect(&mut board, create, billing, "charges");
    connect(&mut board, retry, billing, "retries");
    let _ = checkout;
    board
}

fn ids(nodes: impl IntoIterator<Item = NodeId>) -> Vec<String> {
    nodes.into_iter().map(|n| n.to_string()).collect()
}

#[test]
fn a_child_sits_one_level_below_its_parent_unless_told_otherwise() {
    let board = system();

    let altitude = |n: u32| board.nodes[&NodeId(n)].altitude;

    assert_eq!(altitude(3), Some(2));
    assert_eq!(altitude(5), Some(4));
    assert_eq!(board.nodes[&NodeId(3)].parent, Some(NodeId(1)));
}

#[test]
fn a_level_shows_only_what_is_that_high_or_higher() {
    let board = system();

    assert_eq!(ids(view::at(&board, 1).visible), vec!["n1", "n2"]);
    assert_eq!(
        ids(view::at(&board, 2).visible),
        vec!["n1", "n2", "n3", "n4"]
    );
    assert_eq!(
        ids(view::at(&board, 4).visible),
        vec!["n1", "n2", "n3", "n4", "n5"]
    );
}

#[test]
fn nodes_without_an_altitude_show_at_every_level() {
    let mut board = system();
    add(&mut board, "Unplaced", None, None);

    assert!(view::at(&board, 1).visible.contains(&NodeId(6)));
}

#[test]
fn edges_into_hidden_detail_roll_up_to_the_nearest_visible_parent() {
    let board = system();

    let high = view::at(&board, 1);
    let middle = view::at(&board, 2);

    assert_eq!(high.rollups.len(), 1);
    assert_eq!(high.rollups[0].from, NodeId(1));
    assert_eq!(high.rollups[0].to, NodeId(2));
    assert_eq!(high.rollups[0].via, vec![EdgeId(2), EdgeId(3)]);
    assert!(
        middle.rollups.is_empty(),
        "n5 -> n2 rolls into n4 -> n2, which is already drawn"
    );
}

#[test]
fn a_parent_frames_its_children_only_when_they_are_showing() {
    let board = system();

    assert!(view::at(&board, 1).containers.is_empty());
    assert_eq!(ids(view::at(&board, 2).containers), vec!["n1"]);
    assert_eq!(ids(view::at(&board, 4).containers), vec!["n1", "n4"]);
}

#[test]
fn look_at_a_level_lists_what_shows_there_and_the_rolled_up_edges() {
    let board = system();

    let expected = "\
nodes[2]{id,kind,label,by,alt,in}:
  n1,service,Checkout,agent,1,\"\"
  n2,service,Billing,agent,1,\"\"
rollups[1]{from,to,via}:
  n1,n2,e2 e3
";

    assert_eq!(look::board_at(&board, 1), expected);
}

#[test]
fn a_node_can_be_moved_to_another_level_or_parent() {
    let mut board = system();

    let outcome = apply(
        &mut board,
        Level {
            id: ItemId::Node(NodeId(5)),
            altitude: 3,
            parent: Some(ItemId::Node(NodeId(2))),
        },
    );
    let again = board
        .plan(&Level {
            id: ItemId::Node(NodeId(5)),
            altitude: 3,
            parent: Some(ItemId::Node(NodeId(2))),
        })
        .unwrap();

    assert!(matches!(
        outcome,
        Outcome::Changed(Change::NodeNested { .. })
    ));
    assert_eq!(board.nodes[&NodeId(5)].altitude, Some(3));
    assert_eq!(board.nodes[&NodeId(5)].parent, Some(NodeId(2)));
    assert!(matches!(again, Outcome::Unchanged(_)));
}

#[test]
fn altitudes_run_from_one_to_four() {
    let board = system();

    let result = board.plan(&Level {
        id: ItemId::Node(NodeId(5)),
        altitude: 5,
        parent: None,
    });

    assert_eq!(result.unwrap_err(), Error::InvalidAltitude(5));
}

#[test]
fn a_node_cannot_sit_inside_itself_or_its_own_detail() {
    let board = system();

    let result = board.plan(&Level {
        id: ItemId::Node(NodeId(1)),
        altitude: 1,
        parent: Some(ItemId::Node(NodeId(5))),
    });

    assert_eq!(
        result.unwrap_err(),
        Error::NestingLoop(NodeId(1), NodeId(5))
    );
}

#[test]
fn deleting_a_parent_lifts_its_children_to_the_top() {
    let mut board = system();

    apply(
        &mut board,
        Intent::Delete {
            id: ItemId::Node(NodeId(4)),
        },
    );

    assert_eq!(board.nodes[&NodeId(5)].parent, None);
}

#[test]
fn describing_nesting_says_where_things_sit() {
    let board = system();
    let change = board
        .plan(&Add {
            kind: NodeKind::Component,
            label: "Ledger".into(),
            placement: Placement::Auto,
            altitude: None,
            parent: Some(ItemId::Node(NodeId(2))),
        })
        .unwrap();
    let Outcome::Changed(change) = change else {
        panic!()
    };

    assert_eq!(
        look::describe(&change),
        "added component n6 \"Ledger\" in n2 at altitude 2"
    );
}
