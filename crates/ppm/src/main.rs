//! `ppm`: the CLI, terminal UI, and protocol server.

mod commands;
mod format;
mod hub;
mod palette;
mod serve;
mod stdio;
mod tui;

use clap::{Parser, Subcommand};
use hub::Hub;
use std::io::{self, IsTerminal};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "ppm",
    version,
    about = "See every dev server running on your machine. Run with no command for the terminal UI."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// List servers once
    List {
        /// Print the snapshot as JSON
        #[arg(long)]
        json: bool,
    },
    /// Print a snapshot on every change
    Watch {
        /// One JSON snapshot per line
        #[arg(long, required = true)]
        jsonl: bool,
    },
    /// Stop the server on a port
    Stop {
        port: u16,
        /// Kill instead of asking it to quit
        #[arg(long)]
        force: bool,
    },
    /// Stop it, then rerun its command in the folder it started from
    Restart { port: u16 },
    /// Open http://localhost:<port>
    Open { port: u16 },
    /// Stop the servers Clean up suggests
    Clean {
        /// Stop them without asking
        #[arg(long, short)]
        yes: bool,
    },
    /// Speak the ppm protocol on stdin and stdout
    Stdio,
    /// Speak the ppm protocol over HTTP on loopback
    Serve {
        /// Loopback address to listen on
        #[arg(long, default_value = serve::DEFAULT_LISTEN)]
        listen: SocketAddr,
        /// URL clients use to reach this server, such as a Tailscale URL, for the connection code
        #[arg(long)]
        url: Option<String>,
        /// Web origin whose pages may use the server, such as https://dash.example.com (repeatable)
        #[arg(long = "allow-origin", value_name = "ORIGIN")]
        allow_origin: Vec<String>,
    },
    /// Check permissions and platform support
    Doctor,
}

fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("port-process-manager"))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        None if io::stdin().is_terminal() && io::stdout().is_terminal() => {
            tui::run(Hub::start(commands::engine())).map(|()| ExitCode::SUCCESS)
        }
        None => commands::list(false),
        Some(Command::List { json }) => commands::list(json),
        Some(Command::Watch { .. }) => commands::watch(),
        Some(Command::Stop { port, force }) => Ok(commands::stop(port, force)),
        Some(Command::Restart { port }) => Ok(commands::restart(port)),
        Some(Command::Open { port }) => Ok(commands::open(port)),
        Some(Command::Clean { yes }) => commands::clean(yes),
        Some(Command::Stdio) => {
            let hub = Hub::start(commands::engine());
            stdio::run(hub, io::BufReader::new(io::stdin()), io::stdout().lock())
                .map(|()| ExitCode::SUCCESS)
        }
        Some(Command::Serve {
            listen,
            url,
            allow_origin,
        }) => serve(listen, url, allow_origin),
        Some(Command::Doctor) => Ok(commands::doctor(config_dir())),
    };
    match result {
        Ok(code) => code,
        // The reader went away, as with `ppm watch --jsonl | head`.
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ppm: {error}");
            ExitCode::FAILURE
        }
    }
}

fn serve(listen: SocketAddr, url: Option<String>, origins: Vec<String>) -> io::Result<ExitCode> {
    let dir = config_dir().ok_or_else(|| io::Error::other("no config folder for this user"))?;
    let token = serve::token(&dir)?;
    let listener = serve::bind(listen)?;
    let url = url.unwrap_or_else(|| format!("http://{}", listener.local_addr().unwrap_or(listen)));
    println!("Listening on http://{listen}");
    println!("Connection code (it holds the token, so keep it private):");
    println!("{}", serve::connection_code(&url, &token));
    let access = serve::Access { token, origins };
    serve::run(Hub::start(commands::engine()), listener, access)?;
    Ok(ExitCode::SUCCESS)
}
