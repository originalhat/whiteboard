use wb_core::layout::{NODE_HEIGHT, NODE_WIDTH};
use wb_core::{
    Actor, Board, Change, Draft, EdgeId, ItemId, NodeId, NodeKind, Point, Relation, StrokePoint,
    Target, marks,
};

/// n1 at the origin, n2 to its right, n3 below n1, and e1 joining n1 to n2.
fn board() -> Board {
    let mut board = Board::default();
    for (n, x, y) in [(1, 0.0, 0.0), (2, 400.0, 0.0), (3, 0.0, 300.0)] {
        let change = Change::NodeAdded {
            id: NodeId(n),
            kind: NodeKind::Service,
            label: format!("n{n}"),
            at: Point::new(x, y),
            attached_to: None,
            altitude: None,
            parent: None,
        };
        board.commit(Draft::new(Actor::Agent, change), 0).unwrap();
    }
    let edge = Change::EdgeAdded {
        id: EdgeId(1),
        from: NodeId(1),
        to: NodeId(2),
        label: None,
    };
    board.commit(Draft::new(Actor::Agent, edge), 0).unwrap();
    board
}

fn stroke(points: &[(f64, f64)]) -> Vec<StrokePoint> {
    points
        .iter()
        .map(|&(x, y)| StrokePoint {
            x,
            y,
            t: 0.0,
            pressure: None,
        })
        .collect()
}

/// An ellipse around a centre, starting at angle 0 and sweeping `sweep` of a full turn.
fn loop_around(cx: f64, cy: f64, rx: f64, ry: f64, sweep: f64) -> Vec<StrokePoint> {
    let steps = 60;
    let points: Vec<(f64, f64)> = (0..=steps)
        .map(|i| {
            let a = std::f64::consts::TAU * sweep * i as f64 / steps as f64;
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect();
    stroke(&points)
}

fn centre(n: f64, m: f64) -> (f64, f64) {
    (n + NODE_WIDTH / 2.0, m + NODE_HEIGHT / 2.0)
}

fn node(n: u32) -> ItemId {
    ItemId::Node(NodeId(n))
}

fn resolved(targets: Vec<Target>) -> Vec<(Relation, Vec<ItemId>)> {
    targets.into_iter().map(|t| (t.relation, t.items)).collect()
}

#[test]
fn a_loop_around_a_node_circles_it() {
    let (cx, cy) = centre(0.0, 0.0);

    let targets = marks::resolve(&board(), &loop_around(cx, cy, 150.0, 80.0, 1.0), 1.0);

    assert_eq!(resolved(targets), vec![(Relation::Encloses, vec![node(1)])]);
}

#[test]
fn a_sloppy_loop_that_does_not_quite_close_still_circles() {
    let (cx, cy) = centre(0.0, 0.0);

    let targets = marks::resolve(&board(), &loop_around(cx, cy, 150.0, 80.0, 0.9), 1.0);

    assert_eq!(resolved(targets), vec![(Relation::Encloses, vec![node(1)])]);
}

#[test]
fn a_loop_that_overshoots_its_start_still_circles() {
    let (cx, cy) = centre(0.0, 0.0);

    let targets = marks::resolve(&board(), &loop_around(cx, cy, 150.0, 80.0, 1.15), 1.0);

    assert_eq!(resolved(targets), vec![(Relation::Encloses, vec![node(1)])]);
}

#[test]
fn circling_two_connected_nodes_names_the_nodes_not_the_edge_between_them() {
    let targets = marks::resolve(&board(), &loop_around(300.0, 40.0, 400.0, 120.0, 1.0), 1.0);

    assert_eq!(
        resolved(targets),
        vec![(Relation::Encloses, vec![node(1), node(2)])]
    );
}

#[test]
fn circling_the_middle_of_an_edge_circles_the_edge() {
    let targets = marks::resolve(&board(), &loop_around(300.0, 40.0, 60.0, 40.0, 1.0), 1.0);

    assert_eq!(
        resolved(targets),
        vec![(Relation::Encloses, vec![ItemId::Edge(EdgeId(1))])]
    );
}

#[test]
fn a_line_from_one_node_to_another_connects_them() {
    let targets = marks::resolve(
        &board(),
        &stroke(&[centre(0.0, 0.0), centre(0.0, 300.0)]),
        1.0,
    );

    assert_eq!(
        resolved(targets),
        vec![(Relation::Connects, vec![node(1), node(3)])]
    );
}

#[test]
fn how_close_a_line_must_end_to_a_node_depends_on_the_zoom() {
    let (x, _) = centre(0.0, 300.0);
    let near_miss = stroke(&[centre(0.0, 0.0), (x, 300.0 - 15.0)]);

    let zoomed_out = marks::resolve(&board(), &near_miss, 1.0);
    let zoomed_in = marks::resolve(&board(), &near_miss, 4.0);

    assert_eq!(
        resolved(zoomed_out),
        vec![(Relation::Connects, vec![node(1), node(3)])]
    );
    assert!(zoomed_in.is_empty());
}

#[test]
fn a_scribble_over_a_node_strikes_it() {
    let (cx, cy) = centre(0.0, 300.0);
    let zigzag: Vec<(f64, f64)> = (0..12)
        .map(|i| {
            (
                cx - 60.0 + i as f64 * 10.0,
                cy + if i % 2 == 0 { -25.0 } else { 25.0 },
            )
        })
        .collect();

    let targets = marks::resolve(&board(), &stroke(&zigzag), 1.0);

    assert_eq!(resolved(targets), vec![(Relation::Strikes, vec![node(3)])]);
}

#[test]
fn a_line_through_a_node_strikes_it() {
    let (_, cy) = centre(0.0, 300.0);

    let targets = marks::resolve(
        &board(),
        &stroke(&[(-30.0, cy + 20.0), (NODE_WIDTH + 30.0, cy - 20.0)]),
        1.0,
    );

    assert_eq!(resolved(targets), vec![(Relation::Strikes, vec![node(3)])]);
}

#[test]
fn a_stroke_in_empty_space_resolves_to_nothing() {
    let empty = marks::resolve(&board(), &loop_around(1000.0, 1000.0, 50.0, 50.0, 1.0), 1.0);
    let squiggle = marks::resolve(&board(), &stroke(&[(900.0, 900.0), (950.0, 980.0)]), 1.0);

    assert!(empty.is_empty());
    assert!(squiggle.is_empty());
}
