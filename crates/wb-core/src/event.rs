use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::id::{AnnotationId, EdgeId, GroupId, ItemId, NodeId};
use crate::model::{Actor, NodeKind, Point, StrokePoint, Target};

pub const FORMAT_VERSION: &str = "wb/0.1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Event {
    pub seq: u64,
    /// Unix time in milliseconds.
    pub at: i64,
    pub actor: Actor,
    pub turn: u32,
    #[serde(flatten)]
    pub change: Change,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// Client-generated id so a retried send is not applied twice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "payload")]
pub enum Change {
    #[serde(rename = "node.added")]
    NodeAdded {
        id: NodeId,
        kind: NodeKind,
        label: String,
        at: Point,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attached_to: Option<ItemId>,
    },
    #[serde(rename = "node.moved")]
    NodeMoved { id: NodeId, to: Point },
    #[serde(rename = "edge.added")]
    EdgeAdded {
        id: EdgeId,
        from: NodeId,
        to: NodeId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    #[serde(rename = "group.added")]
    GroupAdded {
        id: GroupId,
        label: String,
        members: Vec<NodeId>,
    },
    #[serde(rename = "annotation.added")]
    AnnotationAdded {
        id: AnnotationId,
        stroke: Vec<StrokePoint>,
        zoom: f64,
        targets: Vec<Target>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    #[serde(rename = "item.relabeled")]
    ItemRelabeled { id: ItemId, label: String },
    #[serde(rename = "item.deleted")]
    ItemDeleted { id: ItemId },
    #[serde(rename = "message.sent")]
    MessageSent { text: String },
    #[serde(rename = "turn.passed")]
    TurnPassed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

/// Everything about an event except what the log assigns: sequence, time and turn.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    pub actor: Actor,
    pub change: Change,
    pub why: Option<String>,
    pub device: Option<String>,
    pub client_id: Option<String>,
}

impl Draft {
    pub fn new(actor: Actor, change: Change) -> Draft {
        Draft {
            actor,
            change,
            why: None,
            device: None,
            client_id: None,
        }
    }

    pub fn why(mut self, why: Option<String>) -> Draft {
        self.why = why;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BoardMeta {
    pub id: String,
    pub title: String,
    /// Where the board came from: a ticket id or URL. Absent for a freeform idea.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Unix time in milliseconds.
    pub created_at: i64,
}

/// The first line of an exported board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Header {
    pub format: String,
    pub board: BoardMeta,
}

/// One line of an exported board: the header first, then every event in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ExportLine {
    Header(Header),
    Event(Event),
}
