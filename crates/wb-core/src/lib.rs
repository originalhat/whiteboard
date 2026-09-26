//! The whiteboard's semantic model, event log and replay. No I/O lives here.

mod edit;
mod event;
mod id;
pub mod layout;
pub mod look;
pub mod marks;
mod model;
mod replay;
pub mod view;

pub use edit::{Intent, Outcome, Placement, Unchanged};
pub use event::{BoardMeta, Change, Draft, Event, ExportLine, FORMAT_VERSION, Header};
pub use id::{AnnotationId, EdgeId, GroupId, ItemId, NodeId};
pub use model::{
    Actor, Annotation, Board, Edge, Group, Node, NodeKind, Point, Relation, StrokePoint, Target,
};
pub use replay::replay;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("{0} is not a valid id")]
    InvalidId(String),
    #[error("{0} not found")]
    NotFound(ItemId),
    #[error("{0} was already used on this board")]
    IdReused(ItemId),
    #[error("expected event {expected}, got {got}")]
    OutOfSequence { expected: u64, got: u64 },
    #[error("event is for turn {got} but the board is on turn {expected}")]
    WrongTurn { expected: u32, got: u32 },
    #[error("it is not the {}'s turn", .0.as_str())]
    NotYourTurn(Actor),
    #[error("label can't be empty")]
    EmptyLabel,
    #[error("{0} is not a node")]
    NotANode(ItemId),
    #[error("{0} already uses that label")]
    LabelTaken(ItemId),
    #[error("altitude {0} is out of range; use 1 (whole system) to 4 (code)")]
    InvalidAltitude(u8),
    #[error("{0} can't sit inside {1}, which is already part of {0}")]
    NestingLoop(NodeId, NodeId),
}

/// JSON Schema for one line of an exported board.
pub fn schema() -> serde_json::Value {
    let mut schema = schemars::schema_for!(ExportLine);
    schema.insert(
        "title".into(),
        format!("whiteboard export line ({FORMAT_VERSION})").into(),
    );
    serde_json::to_value(schema).expect("a schema is always valid JSON")
}
