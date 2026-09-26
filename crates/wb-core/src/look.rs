//! The board as compact text for the agent to read. Tables follow TOON: a header naming the
//! fields, then one comma-separated row per item.

use std::fmt::Write;

use crate::event::Change;
use crate::id::{ItemId, NodeId};
use crate::model::{Annotation, Board, Node, NodeKind, Relation};
use crate::view;

pub fn board(board: &Board) -> String {
    board_at(board, view::LOWEST)
}

/// The board as it looks at one altitude, with edges into hidden detail rolled up.
pub fn board_at(board: &Board, altitude: u8) -> String {
    if board.is_empty() {
        return "board is empty\n".into();
    }
    let view = view::at(board, altitude);
    let shown = |id: &ItemId| match id {
        ItemId::Node(n) => view.visible.contains(n),
        _ => true,
    };
    let (notes, nodes): (Vec<&Node>, Vec<&Node>) = board
        .nodes
        .values()
        .filter(|n| view.visible.contains(&n.id))
        .filter(|n| n.attached_to.as_ref().is_none_or(shown))
        .partition(|n| n.kind == NodeKind::Note);
    let leveled = board
        .nodes
        .values()
        .any(|n| n.altitude.is_some() || n.parent.is_some());
    let mut out = String::new();
    table(
        &mut out,
        "nodes",
        if leveled {
            "id,kind,label,by,alt,in"
        } else {
            "id,kind,label,by"
        },
        nodes.iter().map(|n| {
            let mut row = vec![
                n.id.to_string(),
                n.kind.as_str().into(),
                cell(&n.label),
                n.by.as_str().into(),
            ];
            if leveled {
                row.push(n.altitude.map_or_else(|| cell(""), |a| a.to_string()));
                row.push(n.parent.map_or_else(|| cell(""), |p| p.to_string()));
            }
            row
        }),
    );
    table(
        &mut out,
        "edges",
        "id,from,to,label",
        board
            .edges
            .values()
            .filter(|e| view.visible.contains(&e.from) && view.visible.contains(&e.to))
            .map(|e| {
                vec![
                    e.id.to_string(),
                    e.from.to_string(),
                    e.to.to_string(),
                    cell(e.label.as_deref().unwrap_or("")),
                ]
            }),
    );
    table(
        &mut out,
        "rollups",
        "from,to,via",
        view.rollups.iter().map(|r| {
            let via: Vec<String> = r.via.iter().map(|e| e.to_string()).collect();
            vec![r.from.to_string(), r.to.to_string(), via.join(" ")]
        }),
    );
    table(
        &mut out,
        "groups",
        "id,label,members",
        board.groups.values().filter_map(|g| {
            let members: Vec<String> = g
                .members
                .iter()
                .filter(|m| view.visible.contains(m))
                .map(|m| m.to_string())
                .collect();
            (!members.is_empty()).then(|| vec![g.id.to_string(), cell(&g.label), members.join(" ")])
        }),
    );
    table(
        &mut out,
        "notes",
        "id,on,text,by",
        notes.iter().map(|n| {
            vec![
                n.id.to_string(),
                n.attached_to.map_or_else(|| cell(""), |t| t.to_string()),
                cell(&n.label),
                n.by.as_str().into(),
            ]
        }),
    );
    if !board.annotations.is_empty() {
        writeln!(out, "annotations[{}]:", board.annotations.len()).unwrap();
        for annotation in board.annotations.values() {
            writeln!(out, "  {}", mark(board, annotation)).unwrap();
        }
    }
    out
}

pub fn counts(board: &Board) -> String {
    let notes = board
        .nodes
        .values()
        .filter(|n| n.kind == NodeKind::Note)
        .count();
    [
        (board.nodes.len() - notes, "node"),
        (board.edges.len(), "edge"),
        (board.groups.len(), "group"),
        (notes, "note"),
        (board.annotations.len(), "annotation"),
    ]
    .iter()
    .map(|(n, noun)| plural(*n, noun))
    .collect::<Vec<_>>()
    .join(", ")
}

pub fn describe(change: &Change) -> String {
    match change {
        Change::NodeAdded {
            id,
            kind,
            label,
            attached_to,
            altitude,
            parent,
            ..
        } => {
            let on = attached_to.map_or(String::new(), |t| format!(" on {t}"));
            let place = nesting(*altitude, *parent);
            format!("added {} {id} {}{on}{place}", kind.as_str(), quoted(label))
        }
        Change::NodeNested {
            id,
            altitude,
            parent,
        } => format!("leveled {id}{}", nesting(*altitude, *parent)),
        Change::NodeMoved { id, .. } => format!("moved {id}"),
        Change::EdgeAdded {
            id,
            from,
            to,
            label,
        } => {
            let label = label
                .as_ref()
                .map_or(String::new(), |l| format!(" {}", quoted(l)));
            format!("connected {id} {from} -> {to}{label}")
        }
        Change::GroupAdded { id, label, members } => {
            let members: Vec<String> = members.iter().map(|m| m.to_string()).collect();
            format!("grouped {id} {}: {}", quoted(label), members.join(" "))
        }
        Change::AnnotationAdded { id, targets, .. } => {
            let marks: Vec<String> = targets
                .iter()
                .map(|t| {
                    let items: Vec<String> = t.items.iter().map(|i| i.to_string()).collect();
                    format!("{} {}", relation_ing(t.relation), items.join(" and "))
                })
                .collect();
            if marks.is_empty() {
                format!("drew {id}")
            } else {
                format!("drew {id} {}", marks.join("; "))
            }
        }
        Change::ItemRelabeled {
            id: id @ ItemId::Annotation(_),
            label,
        } => format!("commented on {id}: {}", quoted(label)),
        Change::ItemRelabeled { id, label } => format!("relabeled {id} {}", quoted(label)),
        Change::ItemDeleted { id } => format!("deleted {id}"),
        Change::MessageSent { text } => format!("said {}", quoted(text)),
        Change::TurnPassed {
            message,
            pointing_at,
        } => {
            let message = message
                .as_ref()
                .map_or(String::new(), |m| format!(": {}", quoted(m)));
            let pointing = if pointing_at.is_empty() {
                String::new()
            } else {
                let ids: Vec<String> = pointing_at.iter().map(|i| i.to_string()).collect();
                format!(" (pointing at {})", ids.join(" "))
            };
            format!("passed the turn{message}{pointing}")
        }
    }
}

fn nesting(altitude: Option<u8>, parent: Option<NodeId>) -> String {
    let parent = parent.map_or(String::new(), |p| format!(" in {p}"));
    let altitude = altitude.map_or(String::new(), |a| format!(" at altitude {a}"));
    parent + &altitude
}

fn relation_ing(relation: Relation) -> &'static str {
    match relation {
        Relation::Encloses => "circling",
        Relation::Connects => "connecting",
        Relation::Strikes => "striking through",
    }
}

pub fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// A string in double quotes with quotes, backslashes and newlines escaped.
pub fn quoted(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

/// A table cell, quoted only when it would otherwise be ambiguous.
pub fn cell(s: &str) -> String {
    let ambiguous = s.is_empty() || s.trim() != s || s.contains([',', '"', '\\', '\n']);
    if ambiguous { quoted(s) } else { s.into() }
}

fn table(out: &mut String, name: &str, fields: &str, rows: impl Iterator<Item = Vec<String>>) {
    let rows: Vec<Vec<String>> = rows.collect();
    if rows.is_empty() {
        return;
    }
    writeln!(out, "{name}[{}]{{{fields}}}:", rows.len()).unwrap();
    for row in rows {
        writeln!(out, "  {}", row.join(",")).unwrap();
    }
}

/// A mark in words, e.g. `a3: human circled n2 "Order Service": "this needs to be async"`.
pub fn mark(board: &Board, annotation: &Annotation) -> String {
    let who = annotation.by.as_str();
    let marks: Vec<String> = annotation
        .targets
        .iter()
        .map(|target| {
            let verb = match target.relation {
                Relation::Encloses => "circled",
                Relation::Connects => "connected",
                Relation::Strikes => "struck through",
            };
            let items: Vec<String> = target.items.iter().map(|i| item(board, *i)).collect();
            format!("{verb} {}", items.join(" and "))
        })
        .collect();
    let what = if marks.is_empty() {
        "drew a stroke".to_string()
    } else {
        marks.join("; ")
    };
    let text = annotation
        .text
        .as_ref()
        .map_or(String::new(), |t| format!(": {}", quoted(t)));
    format!("{}: {who} {what}{text}", annotation.id)
}

/// An item's id with its label, or a note that it's gone.
pub fn item(board: &Board, id: ItemId) -> String {
    if !board.contains(id) {
        return format!("{id} (deleted)");
    }
    match board.label_of(id) {
        Some(label) => format!("{id} {}", quoted(label)),
        None => id.to_string(),
    }
}
