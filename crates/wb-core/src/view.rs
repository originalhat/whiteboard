//! What the board looks like at one altitude: which nodes show, which frame their children, and
//! how edges into hidden detail roll up onto the nearest visible parent.

use std::collections::{BTreeMap, BTreeSet};

use crate::id::{EdgeId, NodeId};
use crate::model::{Board, Node};

pub const LOWEST: u8 = 4;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct View {
    pub visible: BTreeSet<NodeId>,
    /// Visible nodes with visible children, drawn as a frame around them.
    pub containers: BTreeSet<NodeId>,
    pub rollups: Vec<Rollup>,
}

/// An edge standing in for one or more edges whose ends are hidden at this altitude.
#[derive(Debug, Clone, PartialEq)]
pub struct Rollup {
    pub from: NodeId,
    pub to: NodeId,
    pub via: Vec<EdgeId>,
}

pub fn shows(node: &Node, altitude: u8) -> bool {
    node.altitude.is_none_or(|a| a <= altitude)
}

pub fn at(board: &Board, altitude: u8) -> View {
    let visible: BTreeSet<NodeId> = board
        .nodes
        .values()
        .filter(|n| shows(n, altitude))
        .map(|n| n.id)
        .collect();
    let containers = board
        .nodes
        .values()
        .filter(|n| visible.contains(&n.id))
        .filter_map(|n| n.parent.filter(|p| visible.contains(p)))
        .collect();
    let stand_in = |id: NodeId| {
        let mut next = Some(id);
        while let Some(node) = next {
            if visible.contains(&node) {
                return Some(node);
            }
            next = board.nodes.get(&node).and_then(|n| n.parent);
        }
        None
    };
    let drawn: BTreeSet<(NodeId, NodeId)> = board
        .edges
        .values()
        .filter(|e| visible.contains(&e.from) && visible.contains(&e.to))
        .map(|e| (e.from, e.to))
        .collect();
    let mut grouped: BTreeMap<(NodeId, NodeId), Vec<EdgeId>> = BTreeMap::new();
    for edge in board.edges.values() {
        if visible.contains(&edge.from) && visible.contains(&edge.to) {
            continue;
        }
        if let (Some(from), Some(to)) = (stand_in(edge.from), stand_in(edge.to))
            && from != to
            && !drawn.contains(&(from, to))
        {
            grouped.entry((from, to)).or_default().push(edge.id);
        }
    }
    let rollups = grouped
        .into_iter()
        .map(|((from, to), via)| Rollup { from, to, via })
        .collect();
    View {
        visible,
        containers,
        rollups,
    }
}
