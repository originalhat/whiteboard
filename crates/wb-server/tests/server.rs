use std::path::PathBuf;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;
use wb_core::{Actor, Change, Draft, NodeId, NodeKind, Point};
use wb_server::Server;
use wb_store::Store;

struct Fixture {
    _dir: TempDir,
    db: PathBuf,
    token: String,
    router: Router,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("wb.db");
    let mut store = Store::open(&db).unwrap();
    let board = store.create_board("Checkout", Some("LIN-1"), 0).unwrap();
    let token = store.token(&board.id).unwrap();
    let server = Server::open(&db).unwrap();
    let router = server.router();
    server.watch(Duration::from_millis(20));
    Fixture {
        _dir: dir,
        db,
        token,
        router,
    }
}

fn agent_adds(db: &std::path::Path, n: u32, label: &str) {
    let mut store = Store::open(db).unwrap();
    let change = Change::NodeAdded {
        id: NodeId(n),
        kind: NodeKind::Service,
        label: label.into(),
        at: Point::new(0.0, 0.0),
        attached_to: None,
    };
    store
        .append("b1", Draft::new(Actor::Agent, change), 0)
        .unwrap();
}

async fn get(router: &Router, uri: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn op(f: &Fixture, body: Value) -> (StatusCode, Value) {
    let response = f
        .router
        .clone()
        .oneshot(
            Request::post(format!("/api/boards/b1/ops?t={}", f.token))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

struct Sse {
    body: Body,
    buffer: String,
}

impl Sse {
    async fn open(f: &Fixture) -> Sse {
        let response = f
            .router
            .clone()
            .oneshot(
                Request::get(format!("/api/boards/b1/events?t={}", f.token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        Sse {
            body: response.into_body(),
            buffer: String::new(),
        }
    }

    /// The next message's event name and JSON data, skipping keep-alive comments.
    async fn next(&mut self) -> (String, Value) {
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let message: String = self.buffer.drain(..end + 2).collect();
                let field = |name: &str| {
                    message
                        .lines()
                        .find_map(|l| l.strip_prefix(name))
                        .map(|v| v.trim_start().to_string())
                };
                if let Some(data) = field("data:") {
                    let event = field("event:").unwrap_or_default();
                    return (event, serde_json::from_str(&data).unwrap());
                }
                continue;
            }
            let frame = tokio::time::timeout(Duration::from_secs(3), self.body.frame())
                .await
                .expect("no event within 3s")
                .unwrap()
                .unwrap();
            if let Ok(data) = frame.into_data() {
                self.buffer += std::str::from_utf8(&data).unwrap();
            }
        }
    }
}

#[tokio::test]
async fn the_canvas_api_needs_the_boards_token() {
    let f = fixture();

    let (wrong, _) = get(&f.router, "/api/boards/b1/state?t=nope").await;
    let (missing, _) = get(&f.router, "/api/boards/b9/state?t=nope").await;
    let (right, state) = get(&f.router, &format!("/api/boards/b1/state?t={}", f.token)).await;

    assert_eq!(wrong, StatusCode::FORBIDDEN);
    assert_eq!(missing, StatusCode::NOT_FOUND);
    assert_eq!(right, StatusCode::OK);
    assert_eq!(state["meta"]["title"], "Checkout");
    assert_eq!(state["board"]["turn"], 1);
}

#[tokio::test]
async fn a_node_drawn_on_the_canvas_is_recorded_as_the_humans() {
    let f = fixture();

    let (status, reply) = op(
        &f,
        json!({"op": "add_node", "client_id": "c1", "kind": "component", "label": "Retry worker", "at": {"x": 10.0, "y": 20.0}}),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{reply}");
    assert_eq!(reply["event"]["actor"], "human");
    assert_eq!(reply["event"]["client_id"], "c1");
    assert_eq!(reply["event"]["payload"]["id"], "n1");
    let board = Store::open(&f.db).unwrap().load("b1").unwrap();
    assert_eq!(board.nodes[&NodeId(1)].by, Actor::Human);
    assert_eq!(board.nodes[&NodeId(1)].at, Point::new(10.0, 20.0));
}

#[tokio::test]
async fn a_retried_canvas_edit_is_applied_once() {
    let f = fixture();
    let add = json!({"op": "add_node", "client_id": "c1", "kind": "component", "label": "X", "at": {"x": 0.0, "y": 0.0}});

    let (_, first) = op(&f, add.clone()).await;
    let (_, again) = op(&f, add).await;

    assert_eq!(first, again);
    assert_eq!(
        Store::open(&f.db).unwrap().load("b1").unwrap().nodes.len(),
        1
    );
}

#[tokio::test]
async fn canvas_edits_cover_moving_connecting_relabeling_and_deleting() {
    let f = fixture();
    agent_adds(&f.db, 1, "A");
    agent_adds(&f.db, 2, "B");

    let ops = [
        json!({"op": "move", "client_id": "c1", "id": "n1", "to": {"x": 480.0, "y": 0.0}}),
        json!({"op": "connect", "client_id": "c2", "from": "n1", "to": "n2", "label": "calls"}),
        json!({"op": "relabel", "client_id": "c3", "id": "n2", "label": "Billing"}),
        json!({"op": "note", "client_id": "c4", "text": "why?", "at": {"x": 0.0, "y": 300.0}, "on": "n2"}),
        json!({"op": "delete", "client_id": "c5", "id": "e1"}),
    ];
    for body in ops {
        let (status, reply) = op(&f, body).await;
        assert_eq!(status, StatusCode::OK, "{reply}");
    }

    let board = Store::open(&f.db).unwrap().load("b1").unwrap();
    assert_eq!(board.nodes[&NodeId(1)].at, Point::new(480.0, 0.0));
    assert_eq!(board.nodes[&NodeId(2)].label, "Billing");
    assert_eq!(board.nodes[&NodeId(3)].kind, NodeKind::Note);
    assert!(board.edges.is_empty());
}

#[tokio::test]
async fn an_impossible_edit_is_refused_with_a_reason() {
    let f = fixture();

    let (status, reply) = op(
        &f,
        json!({"op": "connect", "client_id": "c1", "from": "n1", "to": "n2"}),
    )
    .await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(reply["error"], "n1 not found");
}

#[tokio::test]
async fn the_human_can_only_hand_over_a_turn_they_hold() {
    let f = fixture();

    let (early, reply) = op(
        &f,
        json!({"op": "pass_turn", "client_id": "c1", "message": "go"}),
    )
    .await;
    let mut store = Store::open(&f.db).unwrap();
    let pass = Draft::new(Actor::Agent, Change::TurnPassed { message: None });
    store.append("b1", pass, 0).unwrap();
    let (ok, _) = op(
        &f,
        json!({"op": "pass_turn", "client_id": "c2", "message": "go"}),
    )
    .await;

    assert_eq!(early, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(reply["error"], "it is not the human's turn");
    assert_eq!(ok, StatusCode::OK);
    assert_eq!(store.load("b1").unwrap().holder, Actor::Agent);
}

#[tokio::test]
async fn the_event_stream_starts_with_the_whole_board_then_follows_every_change() {
    let f = fixture();
    agent_adds(&f.db, 1, "A");
    let mut sse = Sse::open(&f).await;

    let (kind, snapshot) = sse.next().await;
    agent_adds(&f.db, 2, "B");
    let (change_kind, change) = sse.next().await;

    assert_eq!(kind, "snapshot");
    assert_eq!(snapshot["seq"], 1);
    assert_eq!(snapshot["board"]["nodes"]["n1"]["label"], "A");
    assert_eq!(change_kind, "change");
    assert_eq!(change["event"]["seq"], 2);
    assert_eq!(change["board"]["nodes"]["n2"]["label"], "B");
}

#[tokio::test]
async fn edits_arrive_one_at_a_time_even_when_they_land_together() {
    let f = fixture();
    let mut sse = Sse::open(&f).await;
    sse.next().await;

    agent_adds(&f.db, 1, "A");
    agent_adds(&f.db, 2, "B");
    agent_adds(&f.db, 3, "C");
    let mut seqs = Vec::new();
    for _ in 0..3 {
        let (_, change) = sse.next().await;
        seqs.push(change["event"]["seq"].as_u64().unwrap());
    }

    assert_eq!(seqs, vec![1, 2, 3]);
}

#[tokio::test]
async fn the_stream_carries_the_agents_status_and_messages() {
    let f = fixture();
    let mut store = Store::open(&f.db).unwrap();
    store
        .append(
            "b1",
            Draft::new(Actor::Agent, Change::MessageSent { text: "hi".into() }),
            0,
        )
        .unwrap();
    let mut sse = Sse::open(&f).await;

    let (_, snapshot) = sse.next().await;
    store.set_presence("b1", Some("thinking"), 5).unwrap();
    let (kind, presence) = sse.next().await;

    assert_eq!(snapshot["messages"][0]["text"], "hi");
    assert_eq!(snapshot["messages"][0]["actor"], "agent");
    assert_eq!(kind, "presence");
    assert_eq!(presence["presence"]["text"], "thinking");
}

#[tokio::test]
async fn unknown_api_routes_and_files_are_not_found() {
    let f = fixture();

    let (api, _) = get(&f.router, "/api/nothing").await;
    let (file, _) = get(&f.router, "/nothing.js").await;

    assert_eq!(api, StatusCode::NOT_FOUND);
    assert_eq!(file, StatusCode::NOT_FOUND);
}
