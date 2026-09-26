use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::id::{AnnotationId, EdgeId, GroupId, ItemId, NodeId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    Human,
    Agent,
}

impl Actor {
    pub fn other(self) -> Actor {
        match self {
            Actor::Human => Actor::Agent,
            Actor::Agent => Actor::Human,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Actor::Human => "human",
            Actor::Agent => "agent",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Service,
    Db,
    Queue,
    Actor,
    External,
    Component,
    Note,
}

impl NodeKind {
    pub const ALL: [NodeKind; 7] = [
        NodeKind::Service,
        NodeKind::Db,
        NodeKind::Queue,
        NodeKind::Actor,
        NodeKind::External,
        NodeKind::Component,
        NodeKind::Note,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Service => "service",
            NodeKind::Db => "db",
            NodeKind::Queue => "queue",
            NodeKind::Actor => "actor",
            NodeKind::External => "external",
            NodeKind::Component => "component",
            NodeKind::Note => "note",
        }
    }
}

impl std::str::FromStr for NodeKind {
    type Err = String;

    fn from_str(s: &str) -> Result<NodeKind, String> {
        NodeKind::ALL
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| {
                let kinds: Vec<&str> = NodeKind::ALL.iter().map(|k| k.as_str()).collect();
                format!("unknown kind {s:?}; use one of {}", kinds.join(", "))
            })
    }
}

/// A position in board units. Never screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Point {
        Point { x, y }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub label: String,
    pub at: Point,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached_to: Option<ItemId>,
    pub by: Actor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Edge {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub by: Actor,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Group {
    pub id: GroupId,
    pub label: String,
    pub members: Vec<NodeId>,
    pub by: Actor,
}

/// One sampled point of a freehand stroke, serialized as `[x, y, t, pressure?]`.
/// `t` is milliseconds since the stroke started.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokePoint {
    pub x: f64,
    pub y: f64,
    pub t: f64,
    pub pressure: Option<f64>,
}

impl Serialize for StrokePoint {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut values = vec![self.x, self.y, self.t];
        values.extend(self.pressure);
        values.serialize(s)
    }
}

impl<'de> Deserialize<'de> for StrokePoint {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match Vec::<f64>::deserialize(d)?.as_slice() {
            &[x, y, t] => Ok(StrokePoint {
                x,
                y,
                t,
                pressure: None,
            }),
            &[x, y, t, p] => Ok(StrokePoint {
                x,
                y,
                t,
                pressure: Some(p),
            }),
            other => Err(serde::de::Error::invalid_length(
                other.len(),
                &"3 or 4 numbers",
            )),
        }
    }
}

impl JsonSchema for StrokePoint {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "StrokePoint".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "[x, y, t, pressure?] in board units; t is ms since the stroke started",
            "type": "array",
            "items": { "type": "number" },
            "minItems": 3,
            "maxItems": 4
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Relation {
    Encloses,
    Connects,
    Strikes,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Target {
    pub relation: Relation,
    pub items: Vec<ItemId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Annotation {
    pub id: AnnotationId,
    pub stroke: Vec<StrokePoint>,
    /// Canvas zoom level when the stroke was drawn.
    pub zoom: f64,
    pub targets: Vec<Target>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub by: Actor,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub(crate) struct Allocated {
    pub node: u32,
    pub edge: u32,
    pub group: u32,
    pub annotation: u32,
}

/// The board as of some point in its event log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Board {
    pub nodes: BTreeMap<NodeId, Node>,
    pub edges: BTreeMap<EdgeId, Edge>,
    pub groups: BTreeMap<GroupId, Group>,
    pub annotations: BTreeMap<AnnotationId, Annotation>,
    pub turn: u32,
    pub holder: Actor,
    pub last_seq: u64,
    pub(crate) allocated: Allocated,
}

impl Default for Board {
    fn default() -> Self {
        Board {
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
            groups: BTreeMap::new(),
            annotations: BTreeMap::new(),
            turn: 1,
            holder: Actor::Agent,
            last_seq: 0,
            allocated: Allocated::default(),
        }
    }
}

impl Board {
    pub fn contains(&self, id: ItemId) -> bool {
        match id {
            ItemId::Node(id) => self.nodes.contains_key(&id),
            ItemId::Edge(id) => self.edges.contains_key(&id),
            ItemId::Group(id) => self.groups.contains_key(&id),
            ItemId::Annotation(id) => self.annotations.contains_key(&id),
        }
    }

    /// True if the id was handed out at some point, whether or not the item still exists.
    pub fn was_allocated(&self, id: ItemId) -> bool {
        let a = &self.allocated;
        match id {
            ItemId::Node(NodeId(n)) => n >= 1 && n <= a.node,
            ItemId::Edge(EdgeId(n)) => n >= 1 && n <= a.edge,
            ItemId::Group(GroupId(n)) => n >= 1 && n <= a.group,
            ItemId::Annotation(AnnotationId(n)) => n >= 1 && n <= a.annotation,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
            && self.edges.is_empty()
            && self.groups.is_empty()
            && self.annotations.is_empty()
    }

    pub fn label_of(&self, id: ItemId) -> Option<&str> {
        match id {
            ItemId::Node(id) => self.nodes.get(&id).map(|n| n.label.as_str()),
            ItemId::Edge(id) => self.edges.get(&id).and_then(|e| e.label.as_deref()),
            ItemId::Group(id) => self.groups.get(&id).map(|g| g.label.as_str()),
            ItemId::Annotation(id) => self.annotations.get(&id).and_then(|a| a.text.as_deref()),
        }
    }
}
