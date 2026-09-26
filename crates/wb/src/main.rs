mod app;
mod failure;
mod server;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use wb_core::{ItemId, NodeKind, Placement};

use crate::app::App;
use crate::failure::Failure;

const DESCRIPTION: &str = "Shared whiteboard where you and a human design together";

#[derive(Parser)]
#[command(name = "wb", about = DESCRIPTION, disable_help_subcommand = true)]
struct Cli {
    /// Board to act on instead of the active one
    #[arg(long, global = true, env = "WB_BOARD")]
    board: Option<String>,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start a board, or resume the one already linked to --source
    New {
        title: String,
        /// Ticket id or URL the board is about
        #[arg(long)]
        source: Option<String>,
    },
    /// List boards
    Boards,
    /// Make a board active, by id or by source, and print its canvas address
    Open { board: Option<String> },
    /// Show the board as compact text
    Look {
        /// Only what shows at this altitude: 1 is the whole system, 4 is code
        #[arg(long)]
        altitude: Option<u8>,
    },
    /// Add a node
    Add {
        /// service, db, queue, actor, external, component or note
        kind: NodeKind,
        label: String,
        #[command(flatten)]
        place: Place,
        /// How much detail: 1 is the whole system, 4 is code
        #[arg(long)]
        altitude: Option<u8>,
        /// A node this one is a part of; defaults to one level below it
        #[arg(long = "in", value_name = "ID")]
        parent: Option<ItemId>,
        #[command(flatten)]
        why: Why,
    },
    /// Set a node's altitude, optionally inside a parent node
    Level {
        id: ItemId,
        altitude: u8,
        #[arg(long = "in", value_name = "ID")]
        parent: Option<ItemId>,
        #[command(flatten)]
        why: Why,
    },
    /// Add a note, optionally about an item
    Note {
        text: String,
        #[arg(long)]
        on: Option<ItemId>,
        #[command(flatten)]
        why: Why,
    },
    /// Connect two nodes
    Connect {
        from: ItemId,
        to: ItemId,
        label: Option<String>,
        #[command(flatten)]
        why: Why,
    },
    /// Group nodes under a label
    Group {
        label: String,
        #[arg(required = true)]
        members: Vec<ItemId>,
        #[command(flatten)]
        why: Why,
    },
    /// Move a node next to another item
    Move {
        id: ItemId,
        #[command(flatten)]
        place: Place,
        #[command(flatten)]
        why: Why,
    },
    /// Change the label of a node, edge or group
    Relabel {
        id: ItemId,
        label: String,
        #[command(flatten)]
        why: Why,
    },
    /// Delete items
    Delete {
        #[arg(required = true)]
        ids: Vec<ItemId>,
        #[command(flatten)]
        why: Why,
    },
    /// Reply to the human
    Say { text: String },
    /// Hand the turn to the human and wait until they hand it back
    Wait {
        /// Give up after this many seconds, short enough for an agent's shell timeout
        #[arg(long, default_value_t = 110)]
        timeout: u64,
    },
    /// Tell the human what you're doing right now
    Status {
        #[arg(required_unless_present = "clear")]
        text: Option<String>,
        #[arg(long, conflicts_with = "text")]
        clear: bool,
    },
    /// Show recent events
    Log {
        /// Only events after this sequence number
        #[arg(long)]
        since: Option<u64>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Write a board as JSON Lines
    Export {
        board: Option<String>,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Read an exported board in as a new board
    Import { file: PathBuf },
    /// Print the JSON Schema for exported lines
    Schema,
    /// Run the canvas server (wb open starts it for you)
    Serve {
        #[arg(long, env = "WB_PORT", default_value_t = server::DEFAULT_PORT)]
        port: u16,
    },
}

#[derive(Args)]
#[group(multiple = false)]
struct Place {
    #[arg(long, value_name = "ID")]
    near: Option<ItemId>,
    #[arg(long, value_name = "ID")]
    right_of: Option<ItemId>,
    #[arg(long, value_name = "ID")]
    left_of: Option<ItemId>,
    #[arg(long, value_name = "ID")]
    below: Option<ItemId>,
    #[arg(long, value_name = "ID")]
    above: Option<ItemId>,
}

impl Place {
    fn placement(&self) -> Option<Placement> {
        let Place {
            near,
            right_of,
            left_of,
            below,
            above,
        } = *self;
        near.map(Placement::Near)
            .or(right_of.map(Placement::RightOf))
            .or(left_of.map(Placement::LeftOf))
            .or(below.map(Placement::Below))
            .or(above.map(Placement::Above))
    }
}

#[derive(Args)]
struct Why {
    /// A short reason, kept with the change
    #[arg(long)]
    why: Option<String>,
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            let code = if e.use_stderr() { 2 } else { 0 };
            print!("{}", e.render());
            return ExitCode::from(code);
        }
    };
    match run(cli) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(failure) => {
            print!("{failure}");
            ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<String, Failure> {
    if let Some(Command::Serve { port }) = cli.command {
        return serve(port);
    }
    let mut app = App::open(cli.board)?;
    let Some(command) = cli.command else {
        return app.home();
    };
    match command {
        Command::New { title, source } => app.new_board(&title, source.as_deref()),
        Command::Boards => app.boards(),
        Command::Open { board } => app.open_board(board.as_deref()),
        Command::Look { altitude } => app.look(altitude),
        Command::Add {
            kind,
            label,
            place,
            altitude,
            parent,
            why,
        } => app.edit(
            wb_core::Intent::Add {
                kind,
                label,
                placement: place.placement().unwrap_or(Placement::Auto),
                altitude,
                parent,
            },
            why.why,
        ),
        Command::Level {
            id,
            altitude,
            parent,
            why,
        } => app.edit(
            wb_core::Intent::Level {
                id,
                altitude,
                parent,
            },
            why.why,
        ),
        Command::Note { text, on, why } => app.edit(wb_core::Intent::Note { text, on }, why.why),
        Command::Connect {
            from,
            to,
            label,
            why,
        } => app.edit(wb_core::Intent::Connect { from, to, label }, why.why),
        Command::Group {
            label,
            members,
            why,
        } => app.edit(wb_core::Intent::Group { label, members }, why.why),
        Command::Move { id, place, why } => {
            let placement = place.placement().ok_or_else(|| {
                Failure::new("move needs a place")
                    .help(format!("wb move {id} --right-of <id>"))
                    .help("wb move --help")
            })?;
            app.edit(wb_core::Intent::Move { id, placement }, why.why)
        }
        Command::Relabel { id, label, why } => {
            app.edit(wb_core::Intent::Relabel { id, label }, why.why)
        }
        Command::Delete { ids, why } => app.delete(ids, why.why),
        Command::Say { text } => app.say(text),
        Command::Wait { timeout } => app.wait(std::time::Duration::from_secs(timeout)),
        Command::Status { text, .. } => app.status(text.as_deref()),
        Command::Log { since, limit } => app.log(since, limit),
        Command::Export { board, output } => app.export(board, output),
        Command::Import { file } => app.import(&file),
        Command::Schema => Ok(serde_json::to_string_pretty(&wb_core::schema()).unwrap() + "\n"),
        Command::Serve { .. } => unreachable!("handled above"),
    }
}

fn serve(port: u16) -> Result<String, Failure> {
    App::open(None)?;
    let db = app::data_dir().join("wb.db");
    tokio::runtime::Runtime::new()?
        .block_on(wb_server::serve(&db, port))
        .map_err(|e| Failure::new(format!("wb serve on port {port}: {e}")))?;
    Ok(String::new())
}
