use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::Error;

macro_rules! short_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u32);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}{}", $prefix, self.0)
            }
        }

        impl FromStr for $name {
            type Err = Error;

            fn from_str(s: &str) -> Result<Self, Error> {
                s.strip_prefix($prefix)
                    .and_then(|n| n.parse().ok())
                    .map($name)
                    .ok_or_else(|| Error::InvalidId(s.to_string()))
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.collect_str(self)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = String::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                json_schema!({ "type": "string", "pattern": concat!("^", $prefix, "[0-9]+$") })
            }
        }
    };
}

short_id!(NodeId, "n");
short_id!(EdgeId, "e");
short_id!(GroupId, "g");
short_id!(AnnotationId, "a");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ItemId {
    Node(NodeId),
    Edge(EdgeId),
    Group(GroupId),
    Annotation(AnnotationId),
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItemId::Node(id) => id.fmt(f),
            ItemId::Edge(id) => id.fmt(f),
            ItemId::Group(id) => id.fmt(f),
            ItemId::Annotation(id) => id.fmt(f),
        }
    }
}

impl FromStr for ItemId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Error> {
        match s.chars().next() {
            Some('n') => s.parse().map(ItemId::Node),
            Some('e') => s.parse().map(ItemId::Edge),
            Some('g') => s.parse().map(ItemId::Group),
            Some('a') => s.parse().map(ItemId::Annotation),
            _ => Err(Error::InvalidId(s.to_string())),
        }
    }
}

impl Serialize for ItemId {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ItemId {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for ItemId {
    fn schema_name() -> Cow<'static, str> {
        "ItemId".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "string", "pattern": "^[nega][0-9]+$" })
    }
}

impl From<NodeId> for ItemId {
    fn from(id: NodeId) -> Self {
        ItemId::Node(id)
    }
}

impl From<EdgeId> for ItemId {
    fn from(id: EdgeId) -> Self {
        ItemId::Edge(id)
    }
}

impl From<GroupId> for ItemId {
    fn from(id: GroupId) -> Self {
        ItemId::Group(id)
    }
}

impl From<AnnotationId> for ItemId {
    fn from(id: AnnotationId) -> Self {
        ItemId::Annotation(id)
    }
}
