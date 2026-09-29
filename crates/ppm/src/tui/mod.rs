//! `ppm` with no command: the popover's server list, Clean up and server
//! details in the terminal, ported from WhatThePort's `wtp`.

mod chart;
mod view;

use crate::format;
use crate::machine::{machines_path, Feed, Install, Machine};
use crate::palette::Palette;
use ppm_client::machines;
use ppm_core::protocol::{Call, Event, Server, Snapshot};
use ratatui::crossterm::event::{
    self, Event as Input, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use std::collections::HashSet;
use std::io;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Servers,
    Detail(u16),
    Help,
}

struct Toast {
    text: String,
    error: bool,
    until: Instant,
}

/// A machine the switcher can show. Other machines connect on first view.
struct Slot {
    /// `None` for this machine.
    name: Option<String>,
    state: State,
    /// The latest snapshot and core count while another machine is shown.
    snapshot: Option<Snapshot>,
    cores: u32,
}

enum State {
    Idle,
    Connecting(Receiver<Result<Machine, String>>),
    Ready(Machine),
    Failed(String),
}

pub struct App {
    machines: Vec<Slot>,
    current: usize,
    palette: Palette,
    snapshot: Option<Snapshot>,
    cores: u32,
    page: Page,
    page_before_help: Page,
    selected: Option<u16>,
    selected_index: usize,
    cleaning: bool,
    selection: HashSet<u16>,
    confirming_stop: Option<u16>,
    toast: Option<Toast>,
    cpu_all: bool,
    info_expanded: bool,
    processes_expanded: bool,
    list_offset: usize,
    list_scrolling: bool,
    detail_offset: usize,
    help_offset: usize,
    quit: bool,
}

/// Runs the TUI on `machine`. Tab switches to this machine and the saved ones.
pub fn run(machine: Machine) -> io::Result<()> {
    let mut names: Vec<Option<String>> = vec![None];
    let saved = machines_path().and_then(|path| machines::load(&path));
    names.extend(saved.unwrap_or_default().into_iter().map(|m| Some(m.name)));
    if !names.contains(&machine.name) {
        names.push(machine.name.clone());
    }
    let current = names.iter().position(|n| *n == machine.name).unwrap_or(0);
    let mut first = Some(machine);
    let machines = names
        .into_iter()
        .enumerate()
        .map(|(index, name)| Slot {
            name,
            state: match first.take_if(|_| index == current) {
                Some(machine) => State::Ready(machine),
                None => State::Idle,
            },
            snapshot: None,
            cores: 1,
        })
        .collect();
    let mut app = App {
        machines,
        current,
        palette: Palette::detect(),
        snapshot: None,
        cores: 1,
        page: Page::Servers,
        page_before_help: Page::Servers,
        selected: None,
        selected_index: 0,
        cleaning: false,
        selection: HashSet::new(),
        confirming_stop: None,
        toast: None,
        cpu_all: false,
        info_expanded: false,
        processes_expanded: false,
        list_offset: 0,
        list_scrolling: false,
        detail_offset: 0,
        help_offset: 0,
        quit: false,
    };
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();
    // This machine's scanner exits once it has finished any stop or restart.
    for slot in app.machines {
        if let State::Ready(machine) = slot.state {
            machine.close();
        }
    }
    result
}

impl App {
    fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> io::Result<()> {
        while !self.quit {
            terminal.draw(|frame| view::draw(self, frame))?;
            // Uptimes and messages change without a new scan, so redraw at least every second.
            if event::poll(Duration::from_millis(250))? {
                if let Input::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Release {
                        self.handle(key);
                    }
                }
            }
            self.poll_machines();
        }
        Ok(())
    }

    /// Finishes connections, applies the shown machine's events, and keeps
    /// the latest snapshot of the others for when they're shown.
    fn poll_machines(&mut self) {
        for index in 0..self.machines.len() {
            let slot = &mut self.machines[index];
            if let State::Connecting(rx) = &slot.state {
                slot.state = match rx.try_recv() {
                    Ok(Ok(machine)) => State::Ready(machine),
                    Ok(Err(error)) => State::Failed(error),
                    Err(TryRecvError::Empty) => continue,
                    Err(TryRecvError::Disconnected) => State::Failed("Couldn't connect.".into()),
                };
            }
            let State::Ready(machine) = &slot.state else {
                continue;
            };
            let feeds: Vec<Feed> = std::iter::from_fn(|| machine.try_recv()).collect();
            for feed in feeds {
                if index == self.current {
                    self.apply(feed);
                } else {
                    let slot = &mut self.machines[index];
                    match feed {
                        Feed::Event(Event::Hello(hello)) => slot.cores = hello.host.cores.max(1),
                        Feed::Event(Event::Snapshot(snapshot)) => slot.snapshot = Some(snapshot),
                        _ => {}
                    }
                }
            }
        }
    }

    fn apply(&mut self, feed: Feed) {
        let event = match feed {
            Feed::Event(event) => event,
            Feed::Lost(error) => {
                let text = format!("Lost {}: {error}. Reconnecting…", self.label());
                self.show(text, true);
                return;
            }
        };
        match event {
            Event::Hello(hello) => self.cores = hello.host.cores.max(1),
            Event::Snapshot(snapshot) => {
                if self.selected.is_none() {
                    self.selected = snapshot.servers.first().map(|s| s.port);
                }
                self.snapshot = Some(snapshot);
            }
            Event::Result(result) => {
                if let Some(error) = result.error {
                    self.show(error, true);
                }
            }
            Event::Alert(_) => {}
        }
    }

    fn servers(&self) -> &[Server] {
        self.snapshot.as_ref().map_or(&[], |s| &s.servers)
    }

    /// Clean up hides protected servers.
    fn visible(&self) -> Vec<&Server> {
        self.servers()
            .iter()
            .filter(|s| !self.cleaning || !s.protected)
            .collect()
    }

    fn server(&self, port: u16) -> Option<&Server> {
        self.servers().iter().find(|s| s.port == port)
    }

    /// Stable color per server: its place in the port-sorted list.
    fn color_index(&self, port: u16) -> usize {
        self.servers()
            .iter()
            .position(|s| s.port == port)
            .unwrap_or(0)
    }

    fn show(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            error,
            until: Instant::now() + Duration::from_millis(2500),
        });
    }

    fn send(&mut self, call: Call) {
        if let State::Ready(machine) = &mut self.machines[self.current].state {
            machine.call(call);
        }
    }

    /// The shown machine's name.
    fn label(&self) -> &str {
        self.machines[self.current]
            .name
            .as_deref()
            .unwrap_or("this machine")
    }

    /// True when there are other machines to switch to.
    fn switcher(&self) -> bool {
        self.machines.len() > 1
    }

    /// What the list says while the shown machine has no snapshot.
    fn waiting(&self) -> (String, bool) {
        match &self.machines[self.current].state {
            State::Ready(_) => ("Scanning…".into(), false),
            State::Failed(error) => (error.clone(), true),
            _ => (format!("Connecting to {}…", self.label()), false),
        }
    }

    /// Shows the next machine, connecting to it the first time.
    fn next_machine(&mut self) {
        if !self.switcher() {
            return;
        }
        let slot = &mut self.machines[self.current];
        slot.snapshot = self.snapshot.take();
        slot.cores = self.cores;
        self.current = (self.current + 1) % self.machines.len();
        let slot = &mut self.machines[self.current];
        self.snapshot = slot.snapshot.take();
        self.cores = slot.cores;
        if matches!(slot.state, State::Idle | State::Failed(_)) {
            slot.state = match slot.name.clone() {
                None => match Machine::local() {
                    Ok(machine) => State::Ready(machine),
                    Err(error) => State::Failed(error.to_string()),
                },
                Some(name) => {
                    let (tx, rx) = mpsc::channel();
                    thread::spawn(move || {
                        let _ = tx.send(Machine::remote(&name, Install::No));
                    });
                    State::Connecting(rx)
                }
            };
        }
        self.page = Page::Servers;
        self.selected = self.servers().first().map(|s| s.port);
        self.selected_index = 0;
        self.list_offset = 0;
        self.list_scrolling = false;
        self.confirming_stop = None;
        self.toast = None;
    }

    // ------------------------------------------------------------ input

    fn handle(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && matches!(key.code, KeyCode::Char('c')) {
            self.quit = true;
            return;
        }
        if let Some(port) = self.confirming_stop.take() {
            if matches!(key.code, KeyCode::Char('y' | 'Y' | 's') | KeyCode::Enter) {
                self.stop(port);
            }
            return;
        }
        match self.page {
            Page::Help => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.help_offset = self.help_offset.saturating_sub(1)
                }
                KeyCode::Down | KeyCode::Char('j') => self.help_offset += 1,
                _ => self.page = self.page_before_help,
            },
            Page::Servers => self.handle_servers(key.code),
            Page::Detail(port) => match self.server(port).cloned() {
                Some(server) => self.handle_detail(key.code, &server),
                None => self.page = Page::Servers,
            },
        }
    }

    fn handle_servers(&mut self, code: KeyCode) {
        match code {
            KeyCode::PageUp => {
                self.list_scrolling = true;
                self.selected = None;
                self.list_offset = self.list_offset.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.list_scrolling = true;
                self.selected = None;
                self.list_offset = self.list_offset.saturating_add(10);
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Home | KeyCode::Char('g') => {
                self.list_scrolling = false;
                self.list_offset = 0;
                self.selected = self.visible().first().map(|s| s.port)
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.list_scrolling = false;
                self.selected = self.visible().last().map(|s| s.port)
            }
            KeyCode::Char('?') => self.show_help(),
            // Escape steps back a level: out of Clean up, then out of ppm.
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.cleaning {
                    self.set_cleaning(false)
                } else {
                    self.quit = true
                }
            }
            _ if self.cleaning => self.handle_cleaning(code),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                if let Some(port) = self.selected {
                    self.open_detail(port);
                }
            }
            KeyCode::Tab | KeyCode::Char('m') => self.next_machine(),
            KeyCode::Char('c') if !self.servers().is_empty() => self.set_cleaning(true),
            KeyCode::Char('t') => self.cpu_all = !self.cpu_all,
            _ => {
                if let Some(server) = self.selected.and_then(|p| self.server(p)).cloned() {
                    self.shortcut(code, &server);
                }
            }
        }
    }

    fn handle_cleaning(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char(' ' | 'x') => {
                if let Some(port) = self.selected {
                    if !self.selection.remove(&port) {
                        self.selection.insert(port);
                    }
                }
            }
            KeyCode::Char('a') => {
                let all: HashSet<u16> = self.visible().iter().map(|s| s.port).collect();
                self.selection = if self.selection == all {
                    HashSet::new()
                } else {
                    all
                };
            }
            KeyCode::Enter => {
                let picks: Vec<Server> = self
                    .visible()
                    .into_iter()
                    .filter(|s| self.selection.contains(&s.port))
                    .cloned()
                    .collect();
                if picks.is_empty() {
                    return;
                }
                let memory: u64 = picks.iter().map(|s| s.memory).sum();
                for server in &picks {
                    self.send(Call::Stop {
                        port: server.port,
                        root: server.root,
                        force: false,
                        confirm_protected: false,
                    });
                }
                self.set_cleaning(false);
                let count = format::plural(picks.len(), "server", "servers");
                self.show(
                    format!("Stopping {count} · freeing {}", format::bytes(memory)),
                    false,
                );
            }
            KeyCode::Char('c') => self.set_cleaning(false),
            _ => {}
        }
    }

    fn handle_detail(&mut self, code: KeyCode, server: &Server) {
        match code {
            KeyCode::Esc | KeyCode::Left | KeyCode::Backspace | KeyCode::Char('h' | 'q') => {
                self.page = Page::Servers
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.detail_offset = self.detail_offset.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => self.detail_offset += 1,
            KeyCode::Home | KeyCode::Char('g') => self.detail_offset = 0,
            KeyCode::End | KeyCode::Char('G') => self.detail_offset = usize::MAX,
            KeyCode::Tab | KeyCode::BackTab => {
                let ports: Vec<u16> = self.servers().iter().map(|s| s.port).collect();
                if let Some(index) = ports.iter().position(|&p| p == server.port) {
                    let step = if code == KeyCode::Tab {
                        1
                    } else {
                        ports.len() - 1
                    };
                    self.open_detail(ports[(index + step) % ports.len()]);
                }
            }
            KeyCode::Enter => self.open_url(server.port),
            KeyCode::Char('m') => self.next_machine(),
            KeyCode::Char('i') => self.info_expanded = !self.info_expanded,
            KeyCode::Char('p') => self.processes_expanded = !self.processes_expanded,
            KeyCode::Char('?') => self.show_help(),
            _ => self.shortcut(code, server),
        }
    }

    /// The single-key server actions, on the list and in detail.
    fn shortcut(&mut self, code: KeyCode, server: &Server) {
        match code {
            KeyCode::Char('o') => self.open_url(server.port),
            KeyCode::Char('v') => {
                if let Some(url) = server
                    .project
                    .vercel
                    .as_ref()
                    .and_then(|v| v.preview_url.clone())
                {
                    self.open(&url, "Opened the Vercel preview");
                }
            }
            KeyCode::Char('s') => self.confirming_stop = Some(server.port),
            KeyCode::Char('r') => self.restart(server),
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: isize) {
        self.list_scrolling = false;
        let ports: Vec<u16> = self.visible().iter().map(|s| s.port).collect();
        if ports.is_empty() {
            return;
        }
        let next = ports
            .iter()
            .position(|&p| Some(p) == self.selected)
            .map_or(0, |index| {
                (index as isize + delta).clamp(0, ports.len() as isize - 1) as usize
            });
        self.selected = Some(ports[next]);
    }

    fn open_detail(&mut self, port: u16) {
        self.selected = Some(port);
        self.detail_offset = 0;
        self.page = Page::Detail(port);
    }

    fn show_help(&mut self) {
        self.page_before_help = self.page;
        self.help_offset = 0;
        self.page = Page::Help;
    }

    fn set_cleaning(&mut self, cleaning: bool) {
        self.cleaning = cleaning;
        self.list_scrolling = false;
        self.list_offset = 0;
        self.selection = if cleaning {
            self.servers()
                .iter()
                .filter(|s| format::preselected(s))
                .map(|s| s.port)
                .collect()
        } else {
            HashSet::new()
        };
        if cleaning && !self.visible().iter().any(|s| Some(s.port) == self.selected) {
            self.selected = self.visible().first().map(|s| s.port);
        }
    }

    fn stop(&mut self, port: u16) {
        let Some(server) = self.server(port).cloned() else {
            return;
        };
        // The user confirmed, and the prompt named a protected server as such.
        self.send(Call::Stop {
            port,
            root: server.root,
            force: false,
            confirm_protected: true,
        });
        if self.page == Page::Detail(port) {
            self.page = Page::Servers;
        }
        self.show(format!("Stopping {} :{port}", server.project.name), false);
    }

    fn restart(&mut self, server: &Server) {
        if server.command.is_none() || !server.cwd_exists {
            self.show(
                format!(
                    "Can't restart :{}: its command or folder is gone",
                    server.port
                ),
                true,
            );
            return;
        }
        if server.protected {
            let text = format!(
                "{} :{} is protected. Run `ppm restart {} --protected` to restart it.",
                server.process_name, server.port, server.port
            );
            self.show(text, true);
            return;
        }
        self.send(Call::Restart {
            port: server.port,
            root: server.root,
            confirm_protected: false,
        });
        let text = match &server.agent {
            Some(agent) => format!(
                "Restarting :{} outside the {} session",
                server.port,
                format::agent(agent.kind)
            ),
            None => format!("Restarting :{}", server.port),
        };
        self.show(text, false);
    }

    /// Opens a server, forwarding its port first when it runs on another
    /// machine. Forwards stay open until ppm quits.
    fn open_url(&mut self, port: u16) {
        let State::Ready(machine) = &mut self.machines[self.current].state else {
            return;
        };
        match machine.url(port) {
            Ok(url) => {
                let done = match machine.is_local() {
                    true => format!("Opened localhost:{port}"),
                    false => format!("Opened {url}, forwarded from :{port}"),
                };
                self.open(&url, &done);
            }
            Err(error) => self.show(error, true),
        }
    }

    fn open(&mut self, url: &str, done: &str) {
        match open::that_detached(url) {
            Ok(()) => self.show(done, false),
            Err(error) => self.show(format!("Couldn't open {url}: {error}"), true),
        }
    }
}
