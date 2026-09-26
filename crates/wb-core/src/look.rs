//! The board as compact text for the agent to read. Tables follow TOON: a header naming the
//! fields, then one comma-separated row per item.

use std::fmt::Write;

use crate::event::Change;
use crate::id::ItemId;
use crate::model::{Annotation, Board, Node, NodeKind, Relation};

pub fn board(board: &Board) -> String {
    if board.is_empty() {
        return "board is empty\n".into();
    }
    let (notes, nodes): (Vec<&Node>, Vec<&Node>) =
        board.nodes.values().partition(|n| n.kind == NodeKind::Note);
    let mut out = String::new();
    table(
        &mut out,
        "nodes",
        "id,kind,label,by",
        nodes.iter().map(|n| {
            vec![
                n.id.to_string(),
                n.kind.as_str().into(),
                cell(&n.label),
                n.by.as_str().into(),
            ]
        }),
    );
    table(
        &mut out,
        "edges",
        "id,from,to,label",
        board.edges.values().map(|e| {
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
        "groups",
        "id,label,members",
        board.groups.values().map(|g| {
            let members: Vec<String> = g.members.iter().map(|m| m.to_string()).collect();
            vec![g.id.to_string(), cell(&g.label), members.join(" ")]
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
            writeln!(out, "  {}", resolve(board, annotation)).unwrap();
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
            ..
        } => {
            let on = attached_to.map_or(String::new(), |t| format!(" on {t}"));
            format!("added {} {id} {}{on}", kind.as_str(), quoted(label))
        }
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
        Change::AnnotationAdded { id, .. } => format!("drew {id}"),
        Change::ItemRelabeled { id, label } => format!("relabeled {id} {}", quoted(label)),
        Change::ItemDeleted { id } => format!("deleted {id}"),
        Change::MessageSent { text } => format!("said {}", quoted(text)),
        Change::TurnPassed { message: None } => "passed the turn".into(),
        Change::TurnPassed { message: Some(m) } => format!("passed the turn: {}", quoted(m)),
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

fn resolve(board: &Board, annotation: &Annotation) -> String {
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

fn item(board: &Board, id: ItemId) -> String {
    if !board.contains(id) {
        return format!("{id} (deleted)");
    }
    match board.label_of(id) {
        Some(label) => format!("{id} {}", quoted(label)),
        None => id.to_string(),
    }
}
