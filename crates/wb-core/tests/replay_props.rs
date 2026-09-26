use proptest::prelude::*;
use wb_core::{
    Actor, Board, Change, Draft, EdgeId, Event, GroupId, ItemId, NodeId, NodeKind, Point, replay,
};

#[derive(Debug, Clone)]
struct Step {
    op: u8,
    a: usize,
    b: usize,
    human: bool,
}

fn step() -> impl Strategy<Value = Step> {
    (0u8..7, 0usize..16, 0usize..16, any::<bool>()).prop_map(|(op, a, b, human)| Step {
        op,
        a,
        b,
        human,
    })
}

fn pick<T: Copy>(items: &[T], i: usize) -> Option<T> {
    (!items.is_empty()).then(|| items[i % items.len()])
}

fn change_for(board: &Board, step: &Step, next_id: u32) -> Option<Change> {
    let nodes: Vec<NodeId> = board.nodes.keys().copied().collect();
    let items: Vec<ItemId> = board
        .nodes
        .keys()
        .map(|&id| id.into())
        .chain(board.edges.keys().map(|&id| id.into()))
        .chain(board.groups.keys().map(|&id| id.into()))
        .collect();
    match step.op {
        0 => Some(Change::NodeAdded {
            id: NodeId(next_id),
            kind: NodeKind::Service,
            label: format!("node {next_id}"),
            at: Point::new(step.a as f64, step.b as f64),
            attached_to: pick(&items, step.b),
        }),
        1 => Some(Change::EdgeAdded {
            id: EdgeId(next_id),
            from: pick(&nodes, step.a)?,
            to: pick(&nodes, step.b)?,
            label: None,
        }),
        2 => Some(Change::GroupAdded {
            id: GroupId(next_id),
            label: format!("group {next_id}"),
            members: vec![pick(&nodes, step.a)?, pick(&nodes, step.b)?],
        }),
        3 => Some(Change::NodeMoved {
            id: pick(&nodes, step.a)?,
            to: Point::new(step.b as f64, step.a as f64),
        }),
        4 => Some(Change::ItemRelabeled {
            id: pick(&items, step.a)?,
            label: format!("relabeled {}", step.b),
        }),
        5 => Some(Change::ItemDeleted {
            id: pick(&items, step.a)?,
        }),
        _ => Some(Change::TurnPassed { message: None }),
    }
}

fn events_from(steps: &[Step]) -> Vec<Event> {
    let mut board = Board::default();
    let mut events = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        let actor = if step.human {
            Actor::Human
        } else {
            Actor::Agent
        };
        let actor = match step.op {
            6 => board.holder,
            _ => actor,
        };
        let Some(change) = change_for(&board, step, i as u32 + 1) else {
            continue;
        };
        if let Ok(event) = board.commit(Draft::new(actor, change), i as i64) {
            events.push(event);
        }
    }
    events
}

proptest! {
    #[test]
    fn resuming_from_a_snapshot_at_any_point_equals_replaying_everything(
        steps in prop::collection::vec(step(), 0..60),
        cut in any::<prop::sample::Index>(),
    ) {
        let events = events_from(&steps);
        let k = cut.index(events.len() + 1);

        let snapshot = serde_json::to_string(&replay(&events[..k]).unwrap()).unwrap();
        let mut resumed: Board = serde_json::from_str(&snapshot).unwrap();
        for event in &events[k..] {
            resumed.apply(event).unwrap();
        }

        prop_assert_eq!(resumed, replay(&events).unwrap());
    }

    #[test]
    fn events_survive_a_json_round_trip(steps in prop::collection::vec(step(), 0..60)) {
        let events = events_from(&steps);

        let lines: Vec<String> = events.iter().map(|e| serde_json::to_string(e).unwrap()).collect();
        let back: Vec<Event> = lines.iter().map(|l| serde_json::from_str(l).unwrap()).collect();

        prop_assert_eq!(&back, &events);
        prop_assert_eq!(replay(&back).unwrap(), replay(&events).unwrap());
    }
}
