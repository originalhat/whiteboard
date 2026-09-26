use crate::Error;
use crate::event::Change;
use crate::id::{ItemId, NodeId};
use crate::layout::{self, Spot};
use crate::model::{Board, NodeKind, Point};
use crate::replay::check_label;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Auto,
    Near(ItemId),
    RightOf(ItemId),
    LeftOf(ItemId),
    Below(ItemId),
    Above(ItemId),
}

/// What someone wants done to the board, before ids and positions are worked out.
#[derive(Debug, Clone, PartialEq)]
pub enum Intent {
    Add {
        kind: NodeKind,
        label: String,
        placement: Placement,
    },
    Note {
        text: String,
        on: Option<ItemId>,
    },
    Connect {
        from: ItemId,
        to: ItemId,
        label: Option<String>,
    },
    Group {
        label: String,
        members: Vec<ItemId>,
    },
    Move {
        id: ItemId,
        placement: Placement,
    },
    Relabel {
        id: ItemId,
        label: String,
    },
    Delete {
        id: ItemId,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Changed(Change),
    Unchanged(Unchanged),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unchanged {
    Exists(ItemId),
    AlreadyDeleted(ItemId),
    NoChange(ItemId),
}

use Outcome::{Changed, Unchanged as Same};

impl Board {
    /// Works out the change an intent implies, or why there's nothing to do.
    /// Repeating an intent that already happened is a no-op, so retries are safe.
    pub fn plan(&self, intent: &Intent) -> Result<Outcome, Error> {
        match intent {
            Intent::Add {
                kind,
                label,
                placement,
            } => {
                check_label(label)?;
                if let Some(node) = self
                    .nodes
                    .values()
                    .find(|n| n.kind == *kind && n.label == *label)
                {
                    return Ok(Same(Unchanged::Exists(node.id.into())));
                }
                Ok(Changed(Change::NodeAdded {
                    id: self.next_node_id(),
                    kind: *kind,
                    label: label.clone(),
                    at: self.position(*placement, None)?,
                    attached_to: None,
                }))
            }
            Intent::Note { text, on } => {
                check_label(text)?;
                if let Some(target) = on {
                    self.anchor(*target)?;
                }
                if let Some(note) = self
                    .nodes
                    .values()
                    .find(|n| n.kind == NodeKind::Note && n.label == *text && n.attached_to == *on)
                {
                    return Ok(Same(Unchanged::Exists(note.id.into())));
                }
                let placement = on.map_or(Placement::Auto, Placement::Near);
                Ok(Changed(Change::NodeAdded {
                    id: self.next_node_id(),
                    kind: NodeKind::Note,
                    label: text.clone(),
                    at: self.position(placement, None)?,
                    attached_to: *on,
                }))
            }
            Intent::Connect { from, to, label } => {
                let (from, to) = (self.node_id(*from)?, self.node_id(*to)?);
                if let Some(edge) = self
                    .edges
                    .values()
                    .find(|e| e.from == from && e.to == to && e.label == *label)
                {
                    return Ok(Same(Unchanged::Exists(edge.id.into())));
                }
                Ok(Changed(Change::EdgeAdded {
                    id: self.next_edge_id(),
                    from,
                    to,
                    label: label.clone(),
                }))
            }
            Intent::Group { label, members } => {
                check_label(label)?;
                let members = members
                    .iter()
                    .map(|m| self.node_id(*m))
                    .collect::<Result<Vec<_>, _>>()?;
                if let Some(group) = self.groups.values().find(|g| g.label == *label) {
                    return if group.members == members {
                        Ok(Same(Unchanged::Exists(group.id.into())))
                    } else {
                        Err(Error::LabelTaken(group.id.into()))
                    };
                }
                Ok(Changed(Change::GroupAdded {
                    id: self.next_group_id(),
                    label: label.clone(),
                    members,
                }))
            }
            Intent::Move { id, placement } => {
                let id = self.node_id(*id)?;
                let to = self.position(*placement, Some(id))?;
                if to == self.nodes[&id].at {
                    return Ok(Same(Unchanged::NoChange(id.into())));
                }
                Ok(Changed(Change::NodeMoved { id, to }))
            }
            Intent::Relabel { id, label } => {
                if !self.contains(*id) {
                    return Err(Error::NotFound(*id));
                }
                if matches!(id, ItemId::Node(_) | ItemId::Group(_)) {
                    check_label(label)?;
                }
                if self.label_of(*id).unwrap_or("") == label {
                    return Ok(Same(Unchanged::NoChange(*id)));
                }
                Ok(Changed(Change::ItemRelabeled {
                    id: *id,
                    label: label.clone(),
                }))
            }
            Intent::Delete { id } => {
                if self.contains(*id) {
                    Ok(Changed(Change::ItemDeleted { id: *id }))
                } else if self.was_allocated(*id) {
                    Ok(Same(Unchanged::AlreadyDeleted(*id)))
                } else {
                    Err(Error::NotFound(*id))
                }
            }
        }
    }

    fn node_id(&self, id: ItemId) -> Result<NodeId, Error> {
        match id {
            ItemId::Node(n) if self.nodes.contains_key(&n) => Ok(n),
            ItemId::Node(_) => Err(Error::NotFound(id)),
            _ => Err(Error::NotANode(id)),
        }
    }

    fn position(&self, placement: Placement, moving: Option<NodeId>) -> Result<Point, Error> {
        let occupied: Vec<Point> = self
            .nodes
            .values()
            .filter(|n| Some(n.id) != moving)
            .map(|n| n.at)
            .collect();
        let toward = |id, dx, dy| -> Result<Spot, Error> {
            Ok(Spot::Toward {
                from: self.anchor(id)?,
                dx,
                dy,
            })
        };
        let spot = match placement {
            Placement::Auto => Spot::Auto,
            Placement::Near(id) => Spot::Near(self.anchor(id)?),
            Placement::RightOf(id) => toward(id, 1.0, 0.0)?,
            Placement::LeftOf(id) => toward(id, -1.0, 0.0)?,
            Placement::Below(id) => toward(id, 0.0, 1.0)?,
            Placement::Above(id) => toward(id, 0.0, -1.0)?,
        };
        Ok(layout::place(&occupied, spot))
    }

    fn anchor(&self, id: ItemId) -> Result<Point, Error> {
        let not_found = || Error::NotFound(id);
        match id {
            ItemId::Node(n) => self.nodes.get(&n).map(|n| n.at).ok_or_else(not_found),
            ItemId::Edge(e) => {
                let edge = self.edges.get(&e).ok_or_else(not_found)?;
                let (a, b) = (self.nodes[&edge.from].at, self.nodes[&edge.to].at);
                Ok(Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0))
            }
            ItemId::Group(g) => {
                let group = self.groups.get(&g).ok_or_else(not_found)?;
                Ok(group
                    .members
                    .first()
                    .map_or(Point::new(0.0, 0.0), |m| self.nodes[m].at))
            }
            ItemId::Annotation(a) => {
                let annotation = self.annotations.get(&a).ok_or_else(not_found)?;
                Ok(annotation
                    .stroke
                    .first()
                    .map_or(Point::new(0.0, 0.0), |p| Point::new(p.x, p.y)))
            }
        }
    }
}
