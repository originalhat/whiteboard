//! `wb serve`: the canvas API and the embedded web canvas. It shares the SQLite file with the
//! `wb` CLI and watches it, so agent edits reach every open canvas one event at a time.

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{StatusCode, header};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{self, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use wb_core::{Actor, Board, Change, Draft, ItemId, NodeId, NodeKind, Point, StrokePoint, marks};
use wb_store::{Error as StoreError, Store};

mod assets;

pub async fn serve(db: &Path, port: u16) -> std::io::Result<()> {
    let server = Server::open(db).map_err(std::io::Error::other)?;
    server.watch(Duration::from_millis(100));
    let listener =
        tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await?;
    println!("wb serve: listening on http://127.0.0.1:{port}");
    axum::serve(listener, server.router()).await
}

#[derive(Clone)]
pub struct Server {
    inner: Arc<Inner>,
}

struct Inner {
    store: Mutex<Store>,
    channels: Mutex<HashMap<String, Channel>>,
}

/// Everything a board's open canvases are following.
struct Channel {
    tx: broadcast::Sender<Message>,
    board: Board,
    presence: Option<(String, i64)>,
}

#[derive(Clone, Debug)]
enum Message {
    Change { seq: u64, data: String },
    Presence(String),
}

impl Server {
    pub fn open(db: &Path) -> Result<Server, StoreError> {
        Ok(Server {
            inner: Arc::new(Inner {
                store: Mutex::new(Store::open(db)?),
                channels: Mutex::new(HashMap::new()),
            }),
        })
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/api/health", get(|| async { "wb" }))
            .route("/api/boards/{id}/state", get(state))
            .route("/api/boards/{id}/events", get(events))
            .route("/api/boards/{id}/ops", post(apply_op))
            .merge(assets::router())
            .with_state(self.clone())
    }

    /// Polls the database for changes made by anyone, including the CLI in another process,
    /// and fans them out to the canvases following each board.
    pub fn watch(&self, every: Duration) {
        let server = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(every);
            loop {
                interval.tick().await;
                if let Err(e) = server.publish() {
                    eprintln!("wb serve: {e}");
                }
            }
        });
    }

    fn publish(&self) -> Result<(), StoreError> {
        let store = self.inner.store.lock().unwrap();
        let mut channels = self.inner.channels.lock().unwrap();
        channels.retain(|_, channel| channel.tx.receiver_count() > 0);
        for (id, channel) in channels.iter_mut() {
            for event in store.events(id, channel.board.last_seq)? {
                channel.board.apply(&event)?;
                let data = json!({ "event": event, "board": channel.board }).to_string();
                let _ = channel.tx.send(Message::Change {
                    seq: event.seq,
                    data,
                });
            }
            let presence = store.presence(id)?;
            if presence != channel.presence {
                let data = json!({ "presence": presence_json(&presence) }).to_string();
                let _ = channel.tx.send(Message::Presence(data));
                channel.presence = presence;
            }
        }
        Ok(())
    }

    /// The board as the channel last published it, plus a receiver for everything after.
    fn subscribe(&self, id: &str) -> Result<(Value, broadcast::Receiver<Message>), StoreError> {
        let store = self.inner.store.lock().unwrap();
        let mut channels = self.inner.channels.lock().unwrap();
        let channel = match channels.get_mut(id) {
            Some(channel) => channel,
            None => {
                let channel = Channel {
                    tx: broadcast::channel(256).0,
                    board: store.load(id)?,
                    presence: store.presence(id)?,
                };
                channels.entry(id.to_string()).or_insert(channel)
            }
        };
        let snapshot = snapshot(&store, id, &channel.board, &channel.presence)?;
        Ok((snapshot, channel.tx.subscribe()))
    }
}

fn snapshot(
    store: &Store,
    id: &str,
    board: &Board,
    presence: &Option<(String, i64)>,
) -> Result<Value, StoreError> {
    let events: Vec<wb_core::Event> = store
        .events(id, 0)?
        .into_iter()
        .take_while(|e| e.seq <= board.last_seq)
        .collect();
    let messages: Vec<Value> = events
        .iter()
        .filter_map(|e| {
            let text = match &e.change {
                Change::MessageSent { text } => text.clone(),
                Change::TurnPassed {
                    message: Some(m), ..
                } => m.clone(),
                _ => return None,
            };
            Some(json!({ "seq": e.seq, "turn": e.turn, "actor": e.actor, "text": text }))
        })
        .collect();
    Ok(json!({
        "meta": store.board(id)?,
        "board": board,
        "seq": board.last_seq,
        "presence": presence_json(presence),
        "messages": messages,
        "agent_turn_items": agent_turn_items(board, &events),
    }))
}

/// What the agent added or changed in its most recent turn, so the canvas can show what's new.
/// The first turn is the agent's opening sketch, where everything is new, so it counts for nothing.
fn agent_turn_items(board: &Board, events: &[wb_core::Event]) -> Vec<ItemId> {
    let turn = if board.holder == Actor::Agent {
        board.turn
    } else {
        board.turn - 1
    };
    if turn <= 1 {
        return Vec::new();
    }
    let items: std::collections::BTreeSet<ItemId> = events
        .iter()
        .filter(|e| e.actor == Actor::Agent && e.turn == turn)
        .filter_map(|e| e.change.item())
        .filter(|id| board.contains(*id))
        .collect();
    items.into_iter().collect()
}

fn presence_json(presence: &Option<(String, i64)>) -> Value {
    match presence {
        Some((text, at)) => json!({ "text": text, "at": at }),
        None => Value::Null,
    }
}

#[derive(Deserialize)]
struct Token {
    t: String,
}

enum ApiError {
    NotFound(String),
    Forbidden,
    Refused(String),
    Internal(String),
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> ApiError {
        match e {
            StoreError::BoardNotFound(_) => ApiError::NotFound(e.to_string()),
            StoreError::Core(e) => ApiError::Refused(e.to_string()),
            e => ApiError::Internal(e.to_string()),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            ApiError::NotFound(e) => (StatusCode::NOT_FOUND, e),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "wrong or missing board token".into()),
            ApiError::Refused(e) => (StatusCode::UNPROCESSABLE_ENTITY, e),
            ApiError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, e),
        };
        (status, Json(json!({ "error": error }))).into_response()
    }
}

fn authorize(store: &Store, id: &str, token: &str) -> Result<(), ApiError> {
    if store.token(id)? == token {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

async fn state(
    State(server): State<Server>,
    UrlPath(id): UrlPath<String>,
    Query(token): Query<Token>,
) -> Result<Json<Value>, ApiError> {
    let store = server.inner.store.lock().unwrap();
    authorize(&store, &id, &token.t)?;
    let board = store.load(&id)?;
    Ok(Json(snapshot(&store, &id, &board, &store.presence(&id)?)?))
}

async fn events(
    State(server): State<Server>,
    UrlPath(id): UrlPath<String>,
    Query(token): Query<Token>,
) -> Result<Sse<impl Stream<Item = Result<SseEvent, Infallible>>>, ApiError> {
    authorize(&server.inner.store.lock().unwrap(), &id, &token.t)?;
    let (snapshot, rx) = server.subscribe(&id)?;
    let first = stream::once(async move {
        Ok(SseEvent::default()
            .event("snapshot")
            .data(snapshot.to_string()))
    });
    let rest = BroadcastStream::new(rx).map(|message| {
        Ok(match message {
            Ok(Message::Change { seq, data }) => SseEvent::default()
                .event("change")
                .id(seq.to_string())
                .data(data),
            Ok(Message::Presence(data)) => SseEvent::default().event("presence").data(data),
            Err(_) => SseEvent::default().event("resync").data("{}"),
        })
    });
    Ok(Sse::new(first.chain(rest)).keep_alive(KeepAlive::default()))
}

/// An edit made on the canvas. Positions are board units; ids for new items come from the server.
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Op {
    AddNode {
        client_id: String,
        kind: NodeKind,
        label: String,
        at: Point,
    },
    Note {
        client_id: String,
        text: String,
        at: Point,
        on: Option<ItemId>,
    },
    Move {
        client_id: String,
        id: NodeId,
        to: Point,
    },
    Connect {
        client_id: String,
        from: NodeId,
        to: NodeId,
        label: Option<String>,
    },
    Relabel {
        client_id: String,
        id: ItemId,
        label: String,
    },
    Delete {
        client_id: String,
        id: ItemId,
    },
    Annotate {
        client_id: String,
        stroke: Vec<StrokePoint>,
        zoom: f64,
    },
    PassTurn {
        client_id: String,
        message: Option<String>,
        #[serde(default)]
        pointing_at: Vec<ItemId>,
    },
}

impl Op {
    fn into_draft(self, board: &Board) -> Draft {
        let (client_id, change) = match self {
            Op::AddNode {
                client_id,
                kind,
                label,
                at,
            } => (
                client_id,
                Change::NodeAdded {
                    id: board.next_node_id(),
                    kind,
                    label,
                    at,
                    attached_to: None,
                },
            ),
            Op::Note {
                client_id,
                text,
                at,
                on,
            } => (
                client_id,
                Change::NodeAdded {
                    id: board.next_node_id(),
                    kind: NodeKind::Note,
                    label: text,
                    at,
                    attached_to: on,
                },
            ),
            Op::Move { client_id, id, to } => (client_id, Change::NodeMoved { id, to }),
            Op::Connect {
                client_id,
                from,
                to,
                label,
            } => (
                client_id,
                Change::EdgeAdded {
                    id: board.next_edge_id(),
                    from,
                    to,
                    label,
                },
            ),
            Op::Relabel {
                client_id,
                id,
                label,
            } => (client_id, Change::ItemRelabeled { id, label }),
            Op::Delete { client_id, id } => (client_id, Change::ItemDeleted { id }),
            Op::Annotate {
                client_id,
                stroke,
                zoom,
            } => (
                client_id,
                Change::AnnotationAdded {
                    id: board.next_annotation_id(),
                    targets: marks::resolve(board, &stroke, zoom),
                    stroke,
                    zoom,
                    text: None,
                },
            ),
            Op::PassTurn {
                client_id,
                message,
                pointing_at,
            } => (
                client_id,
                Change::TurnPassed {
                    message,
                    pointing_at,
                },
            ),
        };
        let mut draft = Draft::new(Actor::Human, change);
        draft.client_id = Some(client_id);
        draft
    }
}

async fn apply_op(
    State(server): State<Server>,
    UrlPath(id): UrlPath<String>,
    Query(token): Query<Token>,
    Json(op): Json<Op>,
) -> Result<Json<Value>, ApiError> {
    let mut store = server.inner.store.lock().unwrap();
    authorize(&store, &id, &token.t)?;
    let board = store.load(&id)?;
    let event = store.append(&id, op.into_draft(&board), now())?;
    Ok(Json(json!({ "event": event })))
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

pub(crate) fn content_type(path: &str) -> (header::HeaderName, String) {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    (header::CONTENT_TYPE, mime.to_string())
}
