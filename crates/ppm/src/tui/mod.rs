//! `ppm` with no command: the popover's server list, Clean up and server
//! details in the terminal, ported from WhatThePort's `wtp`.

mod chart;
mod view;

use crate::format;
use crate::hub::Hub;
use crate::palette::Palette;
use ppm_core::protocol::{Call, Event, Request, Server, Snapshot};
use ratatui::crossterm::event::{
    self, Event as Input, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use std::collections::HashSet;
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
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

pub struct App {
    hub: Hub,
    events: Sender<Event>,
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
    detail_offset: usize,
    help_offset: usize,
    next_request: u64,
    quit: bool,
}

pub fn run(hub: Hub) -> io::Result<()> {
    let palette = Palette::detect();
    let (events, rx) = mpsc::channel();
    hub.subscribe(events.clone());
    let mut app = App {
        hub,
        events,
        palette,
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
        detail_offset: 0,
        help_offset: 0,
        next_request: 1,
        quit: false,
    };
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal, &rx);
    ratatui::restore();
    result
}

impl App {
    fn run(
        &mut self,
        terminal: &mut ratatui::DefaultTerminal,
        rx: &Receiver<Event>,
    ) -> io::Result<()> {
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
            while let Ok(event) = rx.try_recv() {
                self.apply(event);
            }
        }
        Ok(())
    }

    fn apply(&mut self, event: Event) {
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

    fn now(&self) -> u64 {
        ppm_core::now_ms()
    }

    fn show(&mut self, text: impl Into<String>, error: bool) {
        self.toast = Some(Toast {
            text: text.into(),
            error,
            until: Instant::now() + Duration::from_millis(2500),
        });
    }

    fn send(&mut self, call: Call) {
        let request = Request {
            id: self.next_request,
            call,
        };
        self.next_request += 1;
        self.hub.call(request, self.events.clone());
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
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Home | KeyCode::Char('g') => {
                self.selected = self.visible().first().map(|s| s.port)
            }
            KeyCode::End | KeyCode::Char('G') => {
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
                    });
                }
                self.set_cleaning(false);
                let count = crate::commands::plural(picks.len(), "server", "servers");
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
        let ports: Vec<u16> = self.visible().iter().map(|s| s.port).collect();
        if ports.is_empty() {
            return;
        }
        let index = ports
            .iter()
            .position(|&p| Some(p) == self.selected)
            .unwrap_or(0);
        let next = (index as isize + delta).clamp(0, ports.len() as isize - 1) as usize;
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
        self.send(Call::Stop {
            port,
            root: server.root,
            force: false,
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
        self.send(Call::Restart {
            port: server.port,
            root: server.root,
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

    fn open_url(&mut self, port: u16) {
        self.open(
            &format!("http://localhost:{port}"),
            &format!("Opened localhost:{port}"),
        );
    }

    fn open(&mut self, url: &str, done: &str) {
        match open::that_detached(url) {
            Ok(()) => self.show(done, false),
            Err(error) => self.show(format!("Couldn't open {url}: {error}"), true),
        }
    }
}
