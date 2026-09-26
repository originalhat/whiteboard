//! SQLite storage for boards. The event log is the source of truth; snapshots are a cache.

use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use wb_core::{Board, BoardMeta, Draft, Event, ExportLine, FORMAT_VERSION, Header};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] wb_core::Error),
    #[error("board {0} not found")]
    BoardNotFound(String),
    #[error("unsupported format {0}; this wb reads {version}", version = wb_core::FORMAT_VERSION)]
    UnsupportedFormat(String),
    #[error("line {line}: {reason}")]
    BadExport { line: usize, reason: String },
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoardSummary {
    pub meta: BoardMeta,
    pub events: u64,
    pub updated_at: i64,
}

pub struct Store {
    conn: Connection,
    snapshot_every: u64,
}

const SCHEMA: &str = "
    PRAGMA journal_mode = WAL;
    PRAGMA foreign_keys = ON;
    CREATE TABLE IF NOT EXISTS boards (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        title TEXT NOT NULL,
        source TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS boards_by_source ON boards (source);
    CREATE TABLE IF NOT EXISTS events (
        board_id INTEGER NOT NULL REFERENCES boards (id),
        seq INTEGER NOT NULL,
        at INTEGER NOT NULL,
        actor TEXT NOT NULL,
        turn INTEGER NOT NULL,
        kind TEXT NOT NULL,
        client_id TEXT,
        body TEXT NOT NULL,
        PRIMARY KEY (board_id, seq)
    );
    CREATE UNIQUE INDEX IF NOT EXISTS events_by_client_id
        ON events (board_id, client_id) WHERE client_id IS NOT NULL;
    CREATE TABLE IF NOT EXISTS snapshots (
        board_id INTEGER PRIMARY KEY REFERENCES boards (id),
        seq INTEGER NOT NULL,
        body TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
";

const DEFAULT_SNAPSHOT_EVERY: u64 = 100;

impl Store {
    pub fn open(path: &Path) -> Result<Store, Error> {
        Store::init(Connection::open(path)?)
    }

    pub fn in_memory() -> Result<Store, Error> {
        Store::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Store, Error> {
        conn.execute_batch(SCHEMA)?;
        Ok(Store {
            conn,
            snapshot_every: DEFAULT_SNAPSHOT_EVERY,
        })
    }

    pub fn snapshot_every(self, n: u64) -> Store {
        Store {
            snapshot_every: n.max(1),
            ..self
        }
    }

    pub fn create_board(
        &mut self,
        title: &str,
        source: Option<&str>,
        now: i64,
    ) -> Result<BoardMeta, Error> {
        insert_board(&self.conn, title, source, now)
    }

    pub fn find_by_source(&self, source: &str) -> Result<Option<BoardMeta>, Error> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, title, source, created_at FROM boards
                 WHERE source = ?1 ORDER BY id DESC LIMIT 1",
                [source],
                meta_from_row,
            )
            .optional()?)
    }

    pub fn board(&self, id: &str) -> Result<BoardMeta, Error> {
        self.conn
            .query_row(
                "SELECT id, title, source, created_at FROM boards WHERE id = ?1",
                [row_id(id)?],
                meta_from_row,
            )
            .optional()?
            .ok_or_else(|| Error::BoardNotFound(id.into()))
    }

    pub fn boards(&self) -> Result<Vec<BoardSummary>, Error> {
        let mut statement = self.conn.prepare(
            "SELECT b.id, b.title, b.source, b.created_at, b.updated_at,
                    (SELECT COUNT(*) FROM events e WHERE e.board_id = b.id)
             FROM boards b ORDER BY b.id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(BoardSummary {
                meta: meta_from_row(row)?,
                updated_at: row.get(4)?,
                events: row.get::<_, i64>(5)? as u64,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn load(&self, id: &str) -> Result<Board, Error> {
        load(&self.conn, id)
    }

    pub fn events(&self, id: &str, after: u64) -> Result<Vec<Event>, Error> {
        self.board(id)?;
        events(&self.conn, row_id(id)?, after)
    }

    /// Appends one edit. A draft carrying a client id that was already stored returns the
    /// stored event instead of applying it again.
    pub fn append(&mut self, id: &str, draft: Draft, now: i64) -> Result<Event, Error> {
        let board_id = row_id(id)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(client_id) = &draft.client_id {
            let existing: Option<String> = tx
                .query_row(
                    "SELECT body FROM events WHERE board_id = ?1 AND client_id = ?2",
                    params![board_id, client_id],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(body) = existing {
                return Ok(serde_json::from_str(&body)?);
            }
        }
        let mut board = load(&tx, id)?;
        let event = board.commit(draft, now)?;
        insert_event(&tx, board_id, &event)?;
        if event.seq % self.snapshot_every == 0 {
            tx.execute(
                "INSERT OR REPLACE INTO snapshots (board_id, seq, body) VALUES (?1, ?2, ?3)",
                params![board_id, event.seq as i64, serde_json::to_string(&board)?],
            )?;
        }
        tx.execute(
            "UPDATE boards SET updated_at = ?2 WHERE id = ?1",
            params![board_id, now],
        )?;
        tx.commit()?;
        Ok(event)
    }

    pub fn drop_snapshots(&mut self) -> Result<(), Error> {
        self.conn.execute("DELETE FROM snapshots", [])?;
        Ok(())
    }

    pub fn active(&self) -> Result<Option<String>, Error> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'active_board'",
                [],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn set_active(&mut self, id: &str) -> Result<(), Error> {
        self.board(id)?;
        self.conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('active_board', ?1)",
            [id],
        )?;
        Ok(())
    }

    /// The board as JSON Lines: a header, then every event in order.
    pub fn export(&self, id: &str) -> Result<String, Error> {
        let header = ExportLine::Header(Header {
            format: FORMAT_VERSION.into(),
            board: self.board(id)?,
        });
        let mut out = serde_json::to_string(&header)? + "\n";
        for event in self.events(id, 0)? {
            out += &(serde_json::to_string(&ExportLine::Event(event))? + "\n");
        }
        Ok(out)
    }

    /// Reads an export back in as a new board, replaying it first so a broken log is refused.
    pub fn import(&mut self, jsonl: &str) -> Result<BoardMeta, Error> {
        let mut lines = jsonl
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty());
        let bad = |line: usize, reason: String| Error::BadExport {
            line: line + 1,
            reason,
        };
        let header = match lines.next() {
            Some((i, line)) => match serde_json::from_str(line) {
                Ok(ExportLine::Header(header)) => header,
                Ok(ExportLine::Event(_)) => return Err(bad(i, "expected a header first".into())),
                Err(e) => return Err(bad(i, e.to_string())),
            },
            None => return Err(bad(0, "the file is empty".into())),
        };
        if header.format != FORMAT_VERSION {
            return Err(Error::UnsupportedFormat(header.format));
        }
        let mut board = Board::default();
        let mut events = Vec::new();
        for (i, line) in lines {
            let event = match serde_json::from_str(line) {
                Ok(ExportLine::Event(event)) => event,
                Ok(ExportLine::Header(_)) => return Err(bad(i, "unexpected second header".into())),
                Err(e) => return Err(bad(i, e.to_string())),
            };
            board.apply(&event).map_err(|e| bad(i, e.to_string()))?;
            events.push(event);
        }
        let tx = self.conn.transaction()?;
        let meta = insert_board(
            &tx,
            &header.board.title,
            header.board.source.as_deref(),
            header.board.created_at,
        )?;
        let board_id = row_id(&meta.id)?;
        for event in &events {
            insert_event(&tx, board_id, event)?;
        }
        tx.commit()?;
        Ok(meta)
    }
}

fn insert_board(
    conn: &Connection,
    title: &str,
    source: Option<&str>,
    now: i64,
) -> Result<BoardMeta, Error> {
    conn.execute(
        "INSERT INTO boards (title, source, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        params![title, source, now],
    )?;
    Ok(BoardMeta {
        id: format!("b{}", conn.last_insert_rowid()),
        title: title.into(),
        source: source.map(Into::into),
        created_at: now,
    })
}

fn insert_event(conn: &Connection, board_id: i64, event: &Event) -> Result<(), Error> {
    let body = serde_json::to_value(event)?;
    conn.execute(
        "INSERT INTO events (board_id, seq, at, actor, turn, kind, client_id, body)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            board_id,
            event.seq as i64,
            event.at,
            event.actor.as_str(),
            event.turn,
            body["kind"].as_str(),
            event.client_id,
            body.to_string(),
        ],
    )?;
    Ok(())
}

fn load(conn: &Connection, id: &str) -> Result<Board, Error> {
    let board_id = row_id(id)?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM boards WHERE id = ?1)",
        [board_id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(Error::BoardNotFound(id.into()));
    }
    let snapshot: Option<String> = conn
        .query_row(
            "SELECT body FROM snapshots WHERE board_id = ?1",
            [board_id],
            |row| row.get(0),
        )
        .optional()?;
    let mut board = snapshot
        .and_then(|body| serde_json::from_str::<Board>(&body).ok())
        .unwrap_or_default();
    for event in events(conn, board_id, board.last_seq)? {
        board.apply(&event)?;
    }
    Ok(board)
}

fn events(conn: &Connection, board_id: i64, after: u64) -> Result<Vec<Event>, Error> {
    let mut statement =
        conn.prepare("SELECT body FROM events WHERE board_id = ?1 AND seq > ?2 ORDER BY seq")?;
    let bodies = statement.query_map(params![board_id, after as i64], |row| {
        row.get::<_, String>(0)
    })?;
    bodies
        .map(|body| Ok(serde_json::from_str(&body?)?))
        .collect()
}

fn meta_from_row(row: &Row) -> rusqlite::Result<BoardMeta> {
    Ok(BoardMeta {
        id: format!("b{}", row.get::<_, i64>(0)?),
        title: row.get(1)?,
        source: row.get(2)?,
        created_at: row.get(3)?,
    })
}

fn row_id(id: &str) -> Result<i64, Error> {
    id.strip_prefix('b')
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| Error::BoardNotFound(id.into()))
}
