use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;
use wb_core::{
    Actor, Change, Draft, Intent, ItemId, NodeId, NodeKind, Outcome, Placement, StrokePoint,
};
use wb_store::Store;

struct Session {
    home: TempDir,
    transcript: String,
}

impl Session {
    fn new() -> Session {
        Session {
            home: tempfile::tempdir().unwrap(),
            transcript: String::new(),
        }
    }

    fn path(&self) -> &Path {
        self.home.path()
    }

    fn run(&mut self, args: &[&str]) -> String {
        let output = Command::cargo_bin("wb")
            .unwrap()
            .args(args)
            .env("WB_HOME", self.path())
            .env("WB_NO_SERVE", "1")
            .env_remove("WB_BOARD")
            .current_dir(self.path())
            .output()
            .unwrap();
        let bin = assert_cmd::cargo::cargo_bin("wb");
        let stdout = String::from_utf8(output.stdout)
            .unwrap()
            .replace(&bin.display().to_string(), "[bin]")
            .replace(&self.path().display().to_string(), "[home]");
        let stdout = redact_tokens(&stdout);
        let shown: Vec<String> = args
            .iter()
            .map(|a| {
                if a.contains(' ') || a.is_empty() {
                    format!("{a:?}")
                } else {
                    a.to_string()
                }
            })
            .collect();
        let code = output.status.code().unwrap();
        let command = format!("wb {}", shown.join(" "));
        self.transcript += &format!("$ {}\n{stdout}[exit {code}]\n\n", command.trim_end());
        stdout
    }
}

fn redact_tokens(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("?t=") {
        out += &rest[..i + 3];
        rest = &rest[i + 3..];
        let len = rest
            .find(|c: char| !c.is_ascii_hexdigit())
            .unwrap_or(rest.len());
        out += "[token]";
        rest = &rest[len..];
    }
    out + rest
}

/// Acts on the board the way the canvas would, as the human.
fn human(home: &Path, intent: Intent) {
    let mut store = Store::open(&home.join("wb.db")).unwrap();
    let board = store.load("b1").unwrap();
    let Outcome::Changed(change) = board.plan(&intent).unwrap() else {
        panic!("{intent:?} changed nothing");
    };
    store
        .append("b1", Draft::new(Actor::Human, change), 0)
        .unwrap();
}

fn human_hands_over(home: &Path, message: &str) {
    human_hands_over_pointing(home, message, vec![]);
}

fn human_hands_over_pointing(home: &Path, message: &str, pointing_at: Vec<ItemId>) {
    let mut store = Store::open(&home.join("wb.db")).unwrap();
    let change = Change::TurnPassed {
        message: Some(message.into()),
        pointing_at,
    };
    store
        .append("b1", Draft::new(Actor::Human, change), 0)
        .unwrap();
}

/// Draws a loop around a node on the canvas, as the human, and comments on it.
fn human_circles(home: &Path, id: NodeId, comment: &str) {
    let mut store = Store::open(&home.join("wb.db")).unwrap();
    let board = store.load("b1").unwrap();
    let at = board.nodes[&id].at;
    let stroke: Vec<StrokePoint> = (0..=40)
        .map(|i| {
            let a = std::f64::consts::TAU * i as f64 / 40.0;
            StrokePoint {
                x: at.x + 100.0 + 160.0 * a.cos(),
                y: at.y + 40.0 + 90.0 * a.sin(),
                t: i as f64 * 16.0,
                pressure: None,
            }
        })
        .collect();
    let mark = board.next_annotation_id();
    let change = Change::AnnotationAdded {
        id: mark,
        targets: wb_core::marks::resolve(&board, &stroke, 1.0),
        stroke,
        zoom: 1.0,
        text: None,
    };
    store
        .append("b1", Draft::new(Actor::Human, change), 0)
        .unwrap();
    let comment = Change::ItemRelabeled {
        id: mark.into(),
        label: comment.into(),
    };
    store
        .append("b1", Draft::new(Actor::Human, comment), 0)
        .unwrap();
}

#[test]
fn an_agent_sketches_a_board_by_command() {
    let mut wb = Session::new();

    wb.run(&[]);
    wb.run(&["new", "Checkout redesign", "--source", "LIN-123"]);
    wb.run(&["add", "service", "Order Service"]);
    wb.run(&[
        "add",
        "db",
        "Orders DB",
        "--right-of",
        "n1",
        "--why",
        "orders are persisted",
    ]);
    wb.run(&["add", "queue", "Order Events", "--below", "n1"]);
    wb.run(&["connect", "n1", "n2", "writes"]);
    wb.run(&["connect", "n1", "n3"]);
    wb.run(&["note", "Sync or async?", "--on", "e2"]);
    wb.run(&["group", "Orders", "n1", "n2"]);
    wb.run(&[
        "say",
        "Here's how checkout works today. What's wrong with it?",
    ]);
    wb.run(&["look"]);
    wb.run(&["log"]);
    wb.run(&[]);

    insta::assert_snapshot!(wb.transcript);
}

#[test]
fn repeated_edits_are_safe_no_ops() {
    let mut wb = Session::new();
    wb.run(&["new", "Retries"]);
    wb.run(&["add", "service", "A"]);
    wb.run(&["add", "service", "B"]);
    wb.run(&["connect", "n1", "n2"]);
    wb.transcript.clear();

    wb.run(&["add", "service", "A"]);
    wb.run(&["connect", "n1", "n2"]);
    wb.run(&["relabel", "n1", "A"]);
    wb.run(&["move", "n2", "--right-of", "n1"]);
    wb.run(&["delete", "e1"]);
    wb.run(&["delete", "e1"]);

    insta::assert_snapshot!(wb.transcript);
}

#[test]
fn editing_reports_what_changed_including_knock_on_effects() {
    let mut wb = Session::new();
    wb.run(&["new", "Edits"]);
    wb.run(&["add", "service", "A"]);
    wb.run(&["add", "service", "B"]);
    wb.run(&["add", "service", "C"]);
    wb.run(&["connect", "n1", "n2"]);
    wb.run(&["connect", "n2", "n3", "calls"]);
    wb.run(&["note", "why B?", "--on", "n2"]);
    wb.transcript.clear();

    wb.run(&["move", "n3", "--below", "n1"]);
    wb.run(&["relabel", "n2", "Billing"]);
    wb.run(&["relabel", "e2", "charges"]);
    wb.run(&["delete", "n2"]);
    wb.run(&["look"]);

    insta::assert_snapshot!(wb.transcript);
}

#[test]
fn errors_are_structured_on_stdout_with_a_next_step() {
    let mut wb = Session::new();

    wb.run(&["look"]);
    wb.run(&["new", "Errors"]);
    wb.run(&["add", "service", "A"]);
    wb.run(&["connect", "n1", "n9"]);
    wb.run(&["connect", "n1", "x9"]);
    wb.run(&["add", "widget", "B"]);
    wb.run(&["add", "service", "B", "--colour", "red"]);
    wb.run(&["move", "n1"]);
    wb.run(&["open", "b7"]);

    insta::assert_snapshot!(wb.transcript);
}

#[test]
fn a_board_linked_to_a_ticket_is_resumed_not_duplicated() {
    let mut wb = Session::new();
    wb.run(&["new", "Checkout", "--source", "LIN-123"]);
    wb.run(&["add", "service", "A"]);
    wb.run(&["new", "Something else"]);
    wb.transcript.clear();

    wb.run(&["new", "Checkout again", "--source", "LIN-123"]);
    wb.run(&["open", "b2"]);
    wb.run(&["open", "LIN-123"]);
    wb.run(&["boards"]);

    insta::assert_snapshot!(wb.transcript);
}

#[test]
fn export_and_import_round_trip_a_board() {
    let mut wb = Session::new();
    wb.run(&["new", "Checkout", "--source", "LIN-123"]);
    wb.run(&["add", "service", "A"]);
    wb.run(&["add", "db", "B", "--right-of", "n1"]);
    wb.run(&["connect", "n1", "n2", "reads"]);
    let original = wb.run(&["look"]);
    wb.transcript.clear();

    wb.run(&["export", "-o", "checkout.wb.jsonl"]);
    wb.run(&["import", "checkout.wb.jsonl"]);
    wb.run(&["open", "b2"]);
    let imported = wb.run(&["look"]);

    insta::assert_snapshot!(wb.transcript);
    let body = |look: &str| look.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert_eq!(body(&imported), body(&original));
}

#[test]
fn the_schema_is_available_from_the_binary() {
    let mut wb = Session::new();

    let schema: serde_json::Value = serde_json::from_str(&wb.run(&["schema"])).unwrap();

    assert_eq!(schema, wb_core::schema());
}

#[test]
fn the_agent_hears_about_changes_the_human_made_while_it_worked() {
    let mut wb = Session::new();
    wb.run(&["new", "Heads up"]);
    wb.run(&["add", "service", "A"]);
    wb.transcript.clear();

    human(
        wb.path(),
        Intent::Add {
            kind: NodeKind::Component,
            label: "Retry worker".into(),
            placement: Placement::Below(ItemId::Node(NodeId(1))),
            altitude: None,
            parent: None,
        },
    );
    wb.run(&["add", "service", "B"]);
    wb.run(&["add", "service", "C"]);

    insta::assert_snapshot!(wb.transcript);
}

#[test]
fn waiting_hands_the_turn_over_and_returns_the_humans_reply() {
    let mut wb = Session::new();
    wb.run(&["new", "Turns"]);
    wb.run(&["add", "service", "Order Service"]);
    wb.run(&["add", "service", "CreatePayment", "--right-of", "n1"]);
    wb.run(&["say", "Here's my first take"]);
    wb.transcript.clear();

    let home = wb.path().to_path_buf();
    let reply = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        human(
            &home,
            Intent::Relabel {
                id: ItemId::Node(NodeId(1)),
                label: "Checkout".into(),
            },
        );
        human_circles(&home, NodeId(2), "retries?");
        human_hands_over_pointing(&home, "is this idempotent?", vec![ItemId::Node(NodeId(2))]);
    });
    wb.run(&["wait", "--timeout", "20"]);
    reply.join().unwrap();
    wb.run(&[]);

    insta::assert_snapshot!(wb.transcript);
}

#[test]
fn waiting_gives_up_politely_when_the_human_is_still_thinking() {
    let mut wb = Session::new();
    wb.run(&["new", "Turns"]);
    wb.transcript.clear();

    wb.run(&["wait", "--timeout", "1"]);
    wb.run(&["wait", "--timeout", "1"]);

    insta::assert_snapshot!(wb.transcript);
}

#[test]
fn status_shows_what_the_agent_is_doing() {
    let mut wb = Session::new();
    wb.run(&["new", "Presence"]);
    wb.transcript.clear();

    wb.run(&["status", "thinking about the queue boundary"]);
    wb.run(&[]);
    wb.run(&["status", "--clear"]);

    insta::assert_snapshot!(wb.transcript);
}

#[test]
fn open_gives_the_canvas_address() {
    let mut wb = Session::new();
    wb.run(&["new", "Canvas"]);
    wb.run(&["new", "Other"]);
    wb.transcript.clear();

    wb.run(&["open", "b1"]);
    wb.run(&["open"]);

    insta::assert_snapshot!(wb.transcript);
}

#[test]
fn waiting_again_after_a_timeout_picks_up_a_handover_that_came_in_meanwhile() {
    let mut wb = Session::new();
    wb.run(&["new", "Turns"]);
    wb.run(&["wait", "--timeout", "1"]);
    human_hands_over(wb.path(), "back to you");
    wb.transcript.clear();

    wb.run(&["wait", "--timeout", "1"]);

    insta::assert_snapshot!(wb.transcript);
}

#[test]
fn the_agent_can_sketch_high_up_then_add_detail_below() {
    let mut wb = Session::new();
    wb.run(&["new", "Altitude"]);
    wb.transcript.clear();

    wb.run(&["add", "service", "Checkout", "--altitude", "1"]);
    wb.run(&[
        "add",
        "service",
        "Billing",
        "--altitude",
        "1",
        "--right-of",
        "n1",
    ]);
    wb.run(&["add", "component", "CreatePayment", "--in", "n1"]);
    wb.run(&["connect", "n3", "n2", "charges"]);
    wb.run(&["add", "component", "Retry", "--in", "n1"]);
    wb.run(&["level", "n4", "3", "--in", "n3"]);
    wb.run(&["look", "--altitude", "1"]);
    wb.run(&["look"]);
    wb.run(&["level", "n4", "9"]);

    insta::assert_snapshot!(wb.transcript);
}
