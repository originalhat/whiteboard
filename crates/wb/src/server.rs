use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::app::data_dir;
use crate::failure::Failure;

pub const DEFAULT_PORT: u16 = 4747;

pub fn port() -> u16 {
    std::env::var("WB_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

fn listening(port: u16) -> bool {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_ok()
}

/// Starts `wb serve` in the background unless something is already listening on the port.
pub fn ensure_running(port: u16) -> Result<(), Failure> {
    if listening(port) {
        return Ok(());
    }
    let log = std::fs::File::create(data_dir().join("serve.log"))?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["serve", "--port", &port.to_string()])
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    command.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if listening(port) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(
        Failure::new(format!("the canvas server didn't start on port {port}"))
            .help(format!("see {}", data_dir().join("serve.log").display()))
            .help(format!("wb serve --port {port}")),
    )
}
