use std::fmt;

use wb_core::Error as CoreError;
use wb_store::Error as StoreError;

use crate::app::help_block;

/// An error the agent can act on: what went wrong, and what to try next.
#[derive(Debug)]
pub struct Failure {
    message: String,
    help: Vec<String>,
}

impl Failure {
    pub fn new(message: impl Into<String>) -> Failure {
        Failure {
            message: message.into(),
            help: Vec::new(),
        }
    }

    pub fn help(mut self, line: impl Into<String>) -> Failure {
        self.help.push(line.into());
        self
    }

    pub fn no_active_board() -> Failure {
        Failure::new("no active board")
            .help("wb new \"<title>\" [--source <ticket>]")
            .help("wb boards")
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "error: {}", self.message)?;
        write!(f, "{}", help_block(&self.help))
    }
}

impl From<CoreError> for Failure {
    fn from(e: CoreError) -> Failure {
        let failure = Failure::new(e.to_string());
        match e {
            CoreError::NotFound(_) | CoreError::NotANode(_) | CoreError::LabelTaken(_) => {
                failure.help("wb look")
            }
            _ => failure,
        }
    }
}

impl From<StoreError> for Failure {
    fn from(e: StoreError) -> Failure {
        match e {
            StoreError::Core(e) => e.into(),
            StoreError::BoardNotFound(_) => Failure::new(e.to_string()).help("wb boards"),
            _ => Failure::new(e.to_string()),
        }
    }
}

impl From<std::io::Error> for Failure {
    fn from(e: std::io::Error) -> Failure {
        Failure::new(e.to_string())
    }
}
