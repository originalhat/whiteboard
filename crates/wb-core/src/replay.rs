use crate::Error;
use crate::event::{Change, Draft, Event};
use crate::id::{ItemId, NodeId};
use crate::model::{Annotation, Board, Edge, Group, Node};

pub fn replay(events: &[Event]) -> Result<Board, Error> {
    let mut board = Board::default();
    for event in events {
        board.apply(event)?;
    }
    Ok(board)
}

impl Board {
    /// Applies one event, or leaves the board untouched and says why it can't.
    pub fn apply(&mut self, event: &Event) -> Result<(), Error> {
        self.check(event)?;
        self.mutate(event);
        Ok(())
    }

    /// Stamps a draft with the next sequence number and the current turn, then applies it.
    pub fn commit(&mut self, draft: Draft, at: i64) -> Result<Event, Error> {
        let event = Event {
            seq: self.last_seq + 1,
            at,
            actor: draft.actor,
            turn: self.turn,
            change: draft.change,
            why: draft.why,
            device: draft.device,
            client_id: draft.client_id,
        };
        self.apply(&event)?;
        Ok(event)
    }

    fn check(&self, event: &Event) -> Result<(), Error> {
        let expected = self.last_seq + 1;
        if event.seq != expected {
            return Err(Error::OutOfSequence {
                expected,
                got: event.seq,
            });
        }
        if event.turn != self.turn {
            return Err(Error::WrongTurn {
                expected: self.turn,
                got: event.turn,
            });
        }
        match &event.change {
            Change::NodeAdded {
                id,
                label,
                attached_to,
                ..
            } => {
                self.check_fresh((*id).into())?;
                check_label(label)?;
                if let Some(target) = attached_to {
                    self.check_exists(*target)?;
                }
            }
            Change::NodeMoved { id, .. } => self.check_exists((*id).into())?,
            Change::EdgeAdded { id, from, to, .. } => {
                self.check_fresh((*id).into())?;
                self.check_exists((*from).into())?;
                self.check_exists((*to).into())?;
            }
            Change::GroupAdded { id, label, members } => {
                self.check_fresh((*id).into())?;
                check_label(label)?;
                for member in members {
                    self.check_exists((*member).into())?;
                }
            }
            Change::AnnotationAdded { id, targets, .. } => {
                self.check_fresh((*id).into())?;
                for item in targets.iter().flat_map(|t| &t.items) {
                    self.check_exists(*item)?;
                }
            }
            Change::ItemRelabeled { id, label } => {
                self.check_exists(*id)?;
                if !matches!(id, ItemId::Edge(_) | ItemId::Annotation(_)) {
                    check_label(label)?;
                }
            }
            Change::ItemDeleted { id } => self.check_exists(*id)?,
            Change::MessageSent { .. } => {}
            Change::TurnPassed { .. } => {
                if event.actor != self.holder {
                    return Err(Error::NotYourTurn(event.actor));
                }
            }
        }
        Ok(())
    }

    fn mutate(&mut self, event: &Event) {
        let by = event.actor;
        self.last_seq = event.seq;
        match event.change.clone() {
            Change::NodeAdded {
                id,
                kind,
                label,
                at,
                attached_to,
            } => {
                self.allocated.node = self.allocated.node.max(id.0);
                self.nodes.insert(
                    id,
                    Node {
                        id,
                        kind,
                        label,
                        at,
                        attached_to,
                        by,
                    },
                );
            }
            Change::NodeMoved { id, to } => {
                if let Some(node) = self.nodes.get_mut(&id) {
                    node.at = to;
                }
            }
            Change::EdgeAdded {
                id,
                from,
                to,
                label,
            } => {
                self.allocated.edge = self.allocated.edge.max(id.0);
                self.edges.insert(
                    id,
                    Edge {
                        id,
                        from,
                        to,
                        label,
                        by,
                    },
                );
            }
            Change::GroupAdded { id, label, members } => {
                self.allocated.group = self.allocated.group.max(id.0);
                self.groups.insert(
                    id,
                    Group {
                        id,
                        label,
                        members,
                        by,
                    },
                );
            }
            Change::AnnotationAdded {
                id,
                stroke,
                zoom,
                targets,
                text,
            } => {
                self.allocated.annotation = self.allocated.annotation.max(id.0);
                self.annotations.insert(
                    id,
                    Annotation {
                        id,
                        stroke,
                        zoom,
                        targets,
                        text,
                        by,
                    },
                );
            }
            Change::ItemRelabeled { id, label } => self.relabel(id, label),
            Change::ItemDeleted { id } => self.delete(id),
            Change::MessageSent { .. } => {}
            Change::TurnPassed { .. } => {
                self.turn += 1;
                self.holder = self.holder.other();
            }
        }
    }

    fn relabel(&mut self, id: ItemId, label: String) {
        let optional = (!label.is_empty()).then(|| label.clone());
        match id {
            ItemId::Node(id) => self.nodes.get_mut(&id).map(|n| n.label = label),
            ItemId::Edge(id) => self.edges.get_mut(&id).map(|e| e.label = optional),
            ItemId::Group(id) => self.groups.get_mut(&id).map(|g| g.label = label),
            ItemId::Annotation(id) => self.annotations.get_mut(&id).map(|a| a.text = optional),
        };
    }

    fn delete(&mut self, id: ItemId) {
        match id {
            ItemId::Node(id) => {
                self.nodes.remove(&id);
                self.edges.retain(|_, e| e.from != id && e.to != id);
                for group in self.groups.values_mut() {
                    group.members.retain(|m| *m != id);
                }
            }
            ItemId::Edge(id) => {
                self.edges.remove(&id);
            }
            ItemId::Group(id) => {
                self.groups.remove(&id);
            }
            ItemId::Annotation(id) => {
                self.annotations.remove(&id);
            }
        }
        let dangling: Vec<NodeId> = self
            .nodes
            .values()
            .filter(|n| n.attached_to.is_some_and(|t| !self.contains(t)))
            .map(|n| n.id)
            .collect();
        for id in dangling {
            if let Some(node) = self.nodes.get_mut(&id) {
                node.attached_to = None;
            }
        }
    }

    fn check_fresh(&self, id: ItemId) -> Result<(), Error> {
        if self.was_allocated(id) {
            Err(Error::IdReused(id))
        } else {
            Ok(())
        }
    }

    fn check_exists(&self, id: ItemId) -> Result<(), Error> {
        if self.contains(id) {
            Ok(())
        } else {
            Err(Error::NotFound(id))
        }
    }
}

pub(crate) fn check_label(label: &str) -> Result<(), Error> {
    if label.trim().is_empty() {
        Err(Error::EmptyLabel)
    } else {
        Ok(())
    }
}
