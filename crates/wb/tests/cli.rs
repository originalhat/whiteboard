use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;

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
            .env_remove("WB_BOARD")
            .current_dir(self.path())
            .output()
            .unwrap();
        let bin = assert_cmd::cargo::cargo_bin("wb");
        let stdout = String::from_utf8(output.stdout)
            .unwrap()
            .replace(&bin.display().to_string(), "[bin]")
            .replace(&self.path().display().to_string(), "[home]");
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
