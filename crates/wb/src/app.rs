use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use wb_core::look::{self, cell, quoted};
use wb_core::{
    Actor, Annotation, Board, BoardMeta, Change, Draft, Event, Intent, ItemId, NodeKind, Outcome,
    Placement, Unchanged,
};
use wb_store::{Error as StoreError, Store};

use crate::DESCRIPTION;
use crate::failure::Failure;
use crate::server;

const AGENT: &str = "agent";

type Output = Result<String, Failure>;

pub struct App {
    store: Store,
    board_flag: Option<String>,
}

impl App {
    pub fn open(board_flag: Option<String>) -> Result<App, Failure> {
        let dir = data_dir();
        std::fs::create_dir_all(&dir)?;
        Ok(App {
            store: Store::open(&dir.join("wb.db"))?,
            board_flag,
        })
    }

    pub fn home(&self) -> Output {
        let bin = std::env::current_exe().map_or("wb".into(), |p| p.display().to_string());
        let mut out = format!("bin: {bin}\ndescription: {DESCRIPTION}\n");
        let Some(id) = self.current()? else {
            out += "board: none\n";
            out += &help_block(&["wb new \"<title>\" [--source <ticket>]", "wb boards"]);
            return Ok(out);
        };
        let (meta, board) = (self.store.board(&id)?, self.store.load(&id)?);
        out += &header(&meta, &board);
        if let Some(last) = self
            .store
            .events(&id, board.last_seq.saturating_sub(1))?
            .last()
        {
            out += &format!("last: {}\n", event_row(last));
        }
        if let Some((status, _)) = self.store.presence(&id)? {
            out += &format!("status: {status}\n");
        }
        let unseen = self.unseen_human_changes(&id)?;
        if let Some(first) = unseen.first() {
            out += &format!(
                "unseen: {} (wb log --since {})\n",
                look::plural(unseen.len(), "human change"),
                first.seq - 1
            );
        }
        out += &help_block(&["wb look", "wb add <kind> \"<label>\" [--right-of <id>]"]);
        Ok(out)
    }

    pub fn new_board(&mut self, title: &str, source: Option<&str>) -> Output {
        if let Some(meta) = source
            .map(|s| self.store.find_by_source(s))
            .transpose()?
            .flatten()
        {
            self.store.set_active(&meta.id)?;
            let board = self.store.load(&meta.id)?;
            return Ok(format!(
                "resumed: {}\n{}active: {}\n{}",
                describe_meta(&meta),
                header_status(&board),
                meta.id,
                help_block(&["wb look"])
            ));
        }
        let meta = self.store.create_board(title, source, now())?;
        self.store.set_active(&meta.id)?;
        Ok(format!(
            "created: {}\nactive: {}\n{}",
            describe_meta(&meta),
            meta.id,
            help_block(&["wb add <kind> \"<label>\"", "wb note \"<text>\""])
        ))
    }

    pub fn boards(&self) -> Output {
        let boards = self.store.boards()?;
        if boards.is_empty() {
            return Ok(format!(
                "boards: none\n{}",
                help_block(&["wb new \"<title>\" [--source <ticket>]"])
            ));
        }
        let mut out = format!("boards[{}]{{id,title,source,events}}:\n", boards.len());
        for b in &boards {
            let source = b.meta.source.as_deref().unwrap_or("");
            out += &format!(
                "  {},{},{},{}\n",
                b.meta.id,
                cell(&b.meta.title),
                cell(source),
                b.events
            );
        }
        if let Some(active) = self.store.active()? {
            out += &format!("active: {active}\n");
        }
        out += &help_block(&["wb open <id>"]);
        Ok(out)
    }

    pub fn open_board(&mut self, key: Option<&str>) -> Output {
        let key = match key {
            Some(key) => key.to_string(),
            None => self.board_id()?,
        };
        let key = key.as_str();
        let meta = match self.store.board(key) {
            Ok(meta) => meta,
            Err(StoreError::BoardNotFound(_)) => self
                .store
                .find_by_source(key)?
                .ok_or_else(|| StoreError::BoardNotFound(key.into()))?,
            Err(e) => return Err(e.into()),
        };
        self.store.set_active(&meta.id)?;
        let board = self.store.load(&meta.id)?;
        let port = server::port();
        if std::env::var_os("WB_NO_SERVE").is_none() {
            server::ensure_running(port)?;
        }
        Ok(format!(
            "active: {}\n{}canvas: http://127.0.0.1:{port}/b/{}?t={}\n{}",
            describe_meta(&meta),
            header_status(&board),
            meta.id,
            self.store.token(&meta.id)?,
            help_block(&["wb look", "wb wait"])
        ))
    }

    pub fn look(&mut self, altitude: Option<u8>) -> Output {
        let id = self.board_id()?;
        if let Some(altitude) = altitude
            && !(1..=wb_core::view::LOWEST).contains(&altitude)
        {
            return Err(wb_core::Error::InvalidAltitude(altitude).into());
        }
        self.mark_seen(&id)?;
        let (meta, board) = (self.store.board(&id)?, self.store.load(&id)?);
        let mut out = header(&meta, &board);
        if let Some(altitude) = altitude {
            out += &format!("altitude: {altitude} of {}\n", wb_core::view::LOWEST);
        }
        out += &look::board_at(&board, altitude.unwrap_or(wb_core::view::LOWEST));
        if board.is_empty() {
            out += &help_block(&["wb add <kind> \"<label>\"", "wb note \"<text>\""]);
        }
        Ok(out)
    }

    pub fn edit(&mut self, intent: Intent, why: Option<String>) -> Output {
        let id = self.board_id()?;
        let heads_up = self.heads_up(&id)?;
        let board = self.store.load(&id)?;
        let out = match board.plan(&intent)? {
            Outcome::Changed(change) => {
                let event =
                    self.store
                        .append(&id, Draft::new(Actor::Agent, change).why(why), now())?;
                let place = placement_of(&intent).map_or(String::new(), |p| format!(" {p}"));
                let mut out = format!("{}{place}\n", look::describe(&event.change));
                if let Change::NodeAdded { id, kind, .. } = &event.change
                    && *kind != NodeKind::Note
                {
                    out += &help_block(&[
                        format!("wb connect {id} <id> \"<label>\""),
                        format!("wb note \"<text>\" --on {id}"),
                    ]);
                }
                out
            }
            Outcome::Unchanged(unchanged) => format!(
                "unchanged: {}\n",
                describe_unchanged(&board, &intent, unchanged)
            ),
        };
        self.mark_seen(&id)?;
        Ok(heads_up + &out)
    }

    pub fn delete(&mut self, ids: Vec<ItemId>, why: Option<String>) -> Output {
        let id = self.board_id()?;
        let mut board = self.store.load(&id)?;
        for item in &ids {
            if !board.was_allocated(*item) {
                return Err(wb_core::Error::NotFound(*item).into());
            }
        }
        let mut out = self.heads_up(&id)?;
        for item in ids {
            let intent = Intent::Delete { id: item };
            match board.plan(&intent)? {
                Outcome::Changed(change) => {
                    let event = self.store.append(
                        &id,
                        Draft::new(Actor::Agent, change).why(why.clone()),
                        now(),
                    )?;
                    let mut after = board.clone();
                    after.apply(&event)?;
                    out += &format!(
                        "deleted {}{}\n",
                        item_name(&board, item),
                        knock_on(&board, &after, item)
                    );
                    board = after;
                }
                Outcome::Unchanged(unchanged) => {
                    out += &format!(
                        "unchanged: {}\n",
                        describe_unchanged(&board, &intent, unchanged)
                    );
                }
            }
        }
        self.mark_seen(&id)?;
        Ok(out)
    }

    pub fn say(&mut self, text: String) -> Output {
        let id = self.board_id()?;
        let draft = Draft::new(Actor::Agent, Change::MessageSent { text });
        let heads_up = self.heads_up(&id)?;
        let event = self.store.append(&id, draft, now())?;
        self.mark_seen(&id)?;
        Ok(format!("{heads_up}{}\n", look::describe(&event.change)))
    }

    pub fn wait(&mut self, timeout: Duration) -> Output {
        let id = self.board_id()?;
        let mut board = self.store.load(&id)?;
        let since = self.store.cursor(&id, AGENT)?;
        let unread_handover = self
            .store
            .events(&id, since)?
            .iter()
            .any(|e| e.actor == Actor::Human && matches!(e.change, Change::TurnPassed { .. }));
        if board.holder == Actor::Agent && !unread_handover {
            let pass = Draft::new(
                Actor::Agent,
                Change::TurnPassed {
                    message: None,
                    pointing_at: vec![],
                },
            );
            self.store.append(&id, pass, now())?;
            self.store.set_presence(&id, None, now())?;
            board = self.store.load(&id)?;
        }
        let deadline = Instant::now() + timeout;
        while board.holder != Actor::Agent {
            if Instant::now() >= deadline {
                return Ok(format!(
                    "waiting: still the human's turn after {}s\n{}",
                    timeout.as_secs(),
                    help_block(&["wb wait"])
                ));
            }
            std::thread::sleep(Duration::from_millis(200));
            if self.store.latest_seq(&id)? != board.last_seq {
                board = self.store.load(&id)?;
            }
        }
        let events = self.store.events(&id, since)?;
        let handover = events
            .iter()
            .rev()
            .find_map(|e| match (&e.actor, &e.change) {
                (
                    Actor::Human,
                    Change::TurnPassed {
                        message,
                        pointing_at,
                    },
                ) => Some((message.clone(), pointing_at.clone())),
                _ => None,
            });
        let (message, pointing_at) = handover.unwrap_or_default();
        let changes: Vec<&Event> = events
            .iter()
            .filter(|e| e.actor == Actor::Human && !matches!(e.change, Change::TurnPassed { .. }))
            .collect();
        let new_marks: Vec<&Annotation> = events
            .iter()
            .filter_map(|e| match &e.change {
                Change::AnnotationAdded { id, .. } => board.annotations.get(id),
                _ => None,
            })
            .collect();
        let mut out = format!(
            "turn: {}, agent's turn\nmessage: {}\n",
            board.turn,
            message.as_deref().map_or("none".into(), quoted)
        );
        if !pointing_at.is_empty() {
            let items: Vec<String> = pointing_at.iter().map(|i| look::item(&board, *i)).collect();
            out += &format!("pointing at: {}\n", items.join(", "));
        }
        if !new_marks.is_empty() {
            out += &format!("marks[{}] new this turn:\n", new_marks.len());
            for mark in &new_marks {
                out += &format!("  {}\n", look::mark(&board, mark));
            }
        }
        if changes.is_empty() {
            out += "changes: none\n";
        } else {
            out += &format!("changes[{}] (since seq {since}):\n", changes.len());
            for event in changes {
                out += &format!("  {}\n", event_row(event));
            }
        }
        out += &format!("counts: {}\n", look::counts(&board));
        out += &look::board(&board);
        out += &help_block(&["wb say \"<reply>\"", "wb wait"]);
        self.mark_seen(&id)?;
        Ok(out)
    }

    pub fn status(&mut self, text: Option<&str>) -> Output {
        let id = self.board_id()?;
        self.store.set_presence(&id, text, now())?;
        Ok(match text {
            Some(text) => format!("status: {text}\n"),
            None => "status: cleared\n".into(),
        })
    }

    pub fn log(&self, since: Option<u64>, limit: usize) -> Output {
        let id = self.board_id()?;
        let all = self.store.events(&id, 0)?;
        let total = all.len();
        let shown: Vec<&Event> = match since {
            Some(seq) => all.iter().filter(|e| e.seq > seq).take(limit).collect(),
            None => all.iter().skip(total.saturating_sub(limit)).collect(),
        };
        let (Some(first), Some(last)) = (shown.first(), shown.last()) else {
            return Ok(format!("events: none ({total} total)\n"));
        };
        let mut out = format!(
            "events[{}] ({}-{} of {total}):\n",
            shown.len(),
            first.seq,
            last.seq
        );
        for event in &shown {
            out += &format!("  {}\n", event_row(event));
        }
        if (last.seq as usize) < total {
            out += &help_block(&[format!("wb log --since {}", last.seq)]);
        }
        Ok(out)
    }

    pub fn export(&self, board: Option<String>, output: Option<PathBuf>) -> Output {
        let id = match board {
            Some(id) => id,
            None => self.board_id()?,
        };
        let jsonl = self.store.export(&id)?;
        let Some(path) = output else {
            return Ok(jsonl);
        };
        std::fs::write(&path, &jsonl)?;
        let meta = self.store.board(&id)?;
        Ok(format!(
            "exported: {} to {} ({})\n",
            describe_meta(&meta),
            path.display(),
            look::plural(jsonl.lines().count(), "line")
        ))
    }

    pub fn import(&mut self, file: &Path) -> Output {
        let jsonl = std::fs::read_to_string(file)
            .map_err(|e| Failure::new(format!("can't read {}: {e}", file.display())))?;
        let meta = self.store.import(&jsonl)?;
        let events = self.store.events(&meta.id, 0)?.len();
        Ok(format!(
            "imported: {}, {}\n{}",
            describe_meta(&meta),
            look::plural(events, "event"),
            help_block(&[format!("wb open {}", meta.id)])
        ))
    }

    fn unseen_human_changes(&self, id: &str) -> Result<Vec<Event>, Failure> {
        let since = self.store.cursor(id, AGENT)?;
        Ok(self
            .store
            .events(id, since)?
            .into_iter()
            .filter(|e| e.actor == Actor::Human)
            .collect())
    }

    /// Lists what the human changed since the agent last looked, so it's never overwritten blind.
    fn heads_up(&self, id: &str) -> Result<String, Failure> {
        let unseen = self.unseen_human_changes(id)?;
        if unseen.is_empty() {
            return Ok(String::new());
        }
        let mut out = format!(
            "heads up: the human changed the board since you last looked\nchanges[{}]:\n",
            unseen.len()
        );
        for event in &unseen {
            out += &format!("  {}\n", event_row(event));
        }
        Ok(out)
    }

    fn mark_seen(&mut self, id: &str) -> Result<(), Failure> {
        let latest = self.store.latest_seq(id)?;
        self.store.set_cursor(id, AGENT, latest)?;
        Ok(())
    }

    fn current(&self) -> Result<Option<String>, Failure> {
        match &self.board_flag {
            Some(id) => Ok(Some(id.clone())),
            None => Ok(self.store.active()?),
        }
    }

    fn board_id(&self) -> Result<String, Failure> {
        self.current()?.ok_or_else(Failure::no_active_board)
    }
}

pub fn help_block<S: AsRef<str>>(lines: &[S]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut out = format!("help[{}]:\n", lines.len());
    for line in lines {
        out += &format!("  {}\n", line.as_ref());
    }
    out
}

pub fn data_dir() -> PathBuf {
    std::env::var_os("WB_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::data_dir().map(|d| d.join("whiteboard")))
        .unwrap_or_else(|| PathBuf::from(".whiteboard"))
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn describe_meta(meta: &BoardMeta) -> String {
    let source = meta
        .source
        .as_ref()
        .map_or(String::new(), |s| format!(" (source {s})"));
    format!("{} {}{source}", meta.id, quoted(&meta.title))
}

fn header(meta: &BoardMeta, board: &Board) -> String {
    format!("board: {}\n{}", describe_meta(meta), header_status(board))
}

fn header_status(board: &Board) -> String {
    format!(
        "turn: {}, {}'s turn\ncounts: {}\n",
        board.turn,
        board.holder.as_str(),
        look::counts(board)
    )
}

fn event_row(event: &Event) -> String {
    let why = event
        .why
        .as_ref()
        .map_or(String::new(), |w| format!(" (why: {w})"));
    format!(
        "{} t{} {}: {}{why}",
        event.seq,
        event.turn,
        event.actor.as_str(),
        look::describe(&event.change)
    )
}

fn placement_of(intent: &Intent) -> Option<String> {
    let placement = match intent {
        Intent::Add { placement, .. } | Intent::Move { placement, .. } => *placement,
        _ => return None,
    };
    match placement {
        Placement::Auto => None,
        Placement::Near(id) => Some(format!("near {id}")),
        Placement::RightOf(id) => Some(format!("right of {id}")),
        Placement::LeftOf(id) => Some(format!("left of {id}")),
        Placement::Below(id) => Some(format!("below {id}")),
        Placement::Above(id) => Some(format!("above {id}")),
    }
}

fn item_name(board: &Board, id: ItemId) -> String {
    match board.label_of(id) {
        Some(label) => format!("{id} {}", quoted(label)),
        None => id.to_string(),
    }
}

fn describe_unchanged(board: &Board, intent: &Intent, unchanged: Unchanged) -> String {
    match (unchanged, intent) {
        (Unchanged::Exists(id), _) => format!("{} already exists", item_name(board, id)),
        (Unchanged::AlreadyDeleted(id), _) => format!("{id} was already deleted"),
        (Unchanged::NoChange(id), Intent::Relabel { label, .. }) => {
            format!("{id} is already labeled {}", quoted(label))
        }
        (Unchanged::NoChange(id), _) => {
            let place = placement_of(intent).unwrap_or_else(|| "there".into());
            format!("{id} is already {place}")
        }
    }
}

/// What else a delete took with it, so nothing disappears silently.
fn knock_on(before: &Board, after: &Board, deleted: ItemId) -> String {
    let gone = |id: ItemId| id != deleted && before.contains(id) && !after.contains(id);
    let edges: Vec<String> = before
        .edges
        .keys()
        .filter(|&&e| gone(e.into()))
        .map(|e| e.to_string())
        .collect();
    let detached: Vec<String> = before
        .nodes
        .values()
        .filter(|n| {
            n.attached_to.is_some()
                && after
                    .nodes
                    .get(&n.id)
                    .is_some_and(|a| a.attached_to.is_none())
        })
        .map(|n| n.id.to_string())
        .collect();
    let mut parts = Vec::new();
    if !edges.is_empty() {
        parts.push(format!("also removed {}", edges.join(" ")));
    }
    if !detached.is_empty() {
        let noun = if detached.len() == 1 { "note" } else { "notes" };
        parts.push(format!("detached {noun} {}", detached.join(" ")));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" ({})", parts.join("; "))
    }
}
