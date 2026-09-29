//! `ppm`: the CLI, terminal UI, and protocol server.

mod commands;
mod connect;
mod format;
mod hub;
mod machine;
mod palette;
mod remote;
mod serve;
mod stdio;
mod tui;

use clap::{Parser, Subcommand};
use hub::Hub;
use machine::{Install, Machine};
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
    /// Run the command on another machine
    #[arg(long, value_name = "MACHINE")]
    on: Option<String>,
    /// Don't ask: install ppm on the machine, or stop what clean suggests
    #[arg(long, short, global = true)]
    yes: bool,
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
    Clean,
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
    /// Manage remote machines
    Remote {
        #[command(subcommand)]
        command: Remote,
    },
    /// Check permissions and platform support
    Doctor,
    /// Pipe stdin and stdout to a port on this machine, for forwarding
    #[command(hide = true)]
    Connect { port: u16 },
}

#[derive(Subcommand)]
enum Remote {
    /// Save a machine: `ppm remote add devbox -- ssh devbox`, or `--code` from `ppm serve`
    Add {
        /// Name to use with --on
        name: String,
        /// Connection code that `ppm serve` prints
        #[arg(long, value_name = "CODE", conflicts_with = "command")]
        code: Option<String>,
        /// Command prefix that runs a program on the machine
        #[arg(last = true, required_unless_present = "code", value_name = "COMMAND")]
        command: Vec<String>,
    },
    /// List saved and discovered machines, and the ppm installed on each
    List,
    /// Remove a saved machine
    Rm { name: String },
}

fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("port-process-manager"))
}

fn main() -> ExitCode {
    ppm_core::platform::run_helper();
    let cli = Cli::parse();
    let result = run(cli);
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

fn run(cli: Cli) -> io::Result<ExitCode> {
    let install = if cli.yes { Install::Yes } else { Install::Ask };
    let machine = || match &cli.on {
        None => Ok(Machine::local()),
        Some(name) => Machine::remote(name, install).map_err(io::Error::other),
    };
    let tty = io::stdin().is_terminal() && io::stdout().is_terminal();
    match cli.command {
        None if tty => {
            let machine = machine()?;
            tui::run(machine).map(|()| ExitCode::SUCCESS)
        }
        None => commands::list(machine()?, false),
        Some(Command::List { json }) => commands::list(machine()?, json),
        Some(Command::Watch { .. }) => commands::watch(machine()?),
        Some(Command::Stop { port, force }) => commands::stop(machine()?, port, force),
        Some(Command::Restart { port }) => commands::restart(machine()?, port),
        Some(Command::Open { port }) => commands::open(machine()?, port),
        Some(Command::Clean) => commands::clean(machine()?, cli.yes),
        Some(_) if cli.on.is_some() => Err(io::Error::other(
            "--on works with list, watch, stop, restart, open, clean and the terminal UI",
        )),
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
        Some(Command::Remote { command }) => match command {
            Remote::Add {
                name,
                code,
                command,
            } => remote::add(name, code, command),
            Remote::List => remote::list(),
            Remote::Rm { name } => remote::rm(&name),
        },
        Some(Command::Doctor) => Ok(commands::doctor(config_dir())),
        Some(Command::Connect { port }) => connect::run(port),
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
