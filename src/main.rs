use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use serde::Deserialize;
use serde::Serialize;
use std::fs::File;
use std::io::prelude::*;
use std::{error::Error, io};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Alignment, Constraint, Layout},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState, Wrap},
    Frame, Terminal,
};

fn open_url(url: &str) {
    use std::process::Command;

    Command::new("xdg-open")
        .arg(url)
        .output()
        .expect("failed to execute process");
}

#[derive(Clone, Debug)]
struct Params {
    page: u16,
    query: String,
}

impl Params {
    pub fn new() -> Self {
        Self {
            page: 1,
            query: "".to_string(),
        }
    }

    pub fn next_page(&mut self) {
        let page = self.page;
        self.page = if page < 1000 { page + 1 } else { 1000 }
    }

    pub fn prev_page(&mut self) {
        let page = self.page;
        self.page = if page > 1 { self.page - 1 } else { 0 }
    }

    pub fn set_query<S: Into<String> + std::fmt::Display>(&mut self, query: S) {
        self.query = query.to_string();
    }
}

#[derive(Clone)]
struct App {
    state: TableState,
    items: Responses,
    current: Option<usize>,
    last_id: u64,
}

fn get_last_id() -> std::io::Result<u64> {
    let home_dir = dirs::home_dir();
    if let Some(home) = home_dir {
        if let Ok(id) = std::fs::read_to_string(&format!("{}/.nyaa", home.display())) {
            let id = id.trim();
            let id = id.parse::<u64>().unwrap_or(0);
            Ok(id)
        } else {
            Ok(0)
        }
    } else {
        Ok(0)
    }
}

impl App {
    fn new() -> App {
        App {
            state: TableState::default(),
            items: vec![],
            current: None,
            last_id: 0,
        }
    }

    pub fn set_id(&mut self, id: u64) -> std::io::Result<()> {
        self.last_id = id;
        // now we have to write the file
        let home_dir = dirs::home_dir();
        if let Some(home) = home_dir {
            let mut nyaa_file = File::options()
                .create(true)
                .write(true)
                .open(&format!("{}/.nyaa", home.display()))?;
            nyaa_file.write_all(format!("{}", self.last_id).as_bytes())?;
        };

        Ok(())
    }

    pub fn update_items(&mut self, items: Responses) {
        self.items = items;
    }

    pub fn first_item(&mut self) {
        self.current = Some(0);
        self.state.select(Some(0))
    }

    pub fn last_item(&mut self) {
        let last = Some(self.items.len() - 1);
        self.current = last;
        self.state.select(last);
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    self.items.len() - 1
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.current = Some(i);
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    0
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.current = Some(i);
        self.state.select(Some(i));
    }
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Response {
    pub id: String,
    pub name: String,
    pub hash: String,
    pub date: String,
    pub filesize: String,
    pub category: String,
    pub sub_category: String,
    pub magnet: String,
    pub torrent: String,
    pub seeders: String,
    pub leechers: String,
    pub completed: String,
    pub status: String,
}

type Responses = Vec<Response>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.set_id(get_last_id().unwrap());
    let mut params = Params::new();
    let items = get_items(&params).await?;
    app.update_items(items);

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    run_app(&mut terminal, app, &mut params).await?;

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

// fetch the request
async fn get_items(params: &Params) -> Result<Responses, Box<dyn Error>> {
    let client = reqwest::Client::new();
    let query = client
        .get("http://localhost:3000")
        .query(&[("p", params.page.to_string()), ("q", params.query.clone())]);
    let res = query.send().await?.json::<Responses>().await?;

    Ok(res)
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    params: &mut Params,
) -> Result<(), Box<dyn Error>> {
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Down | KeyCode::Char('j') => app.next(),
                KeyCode::Up | KeyCode::Char('k') => app.previous(),
                KeyCode::Char('G') => app.last_item(),
                KeyCode::Char('g') => app.first_item(),
                KeyCode::Char('n') => {
                    params.next_page();
                    let items = get_items(params).await?;
                    app.update_items(items);
                    terminal.draw(|f| ui(f, &mut app))?;
                }
                KeyCode::Char('p') => {
                    params.prev_page();
                    let items = get_items(params).await?;
                    app.update_items(items);
                    terminal.draw(|f| ui(f, &mut app))?;
                }
                KeyCode::Char('/') => {
                    let mut query = String::from("");
                    loop {
                        if let Event::Key(key) = event::read()? {
                            match key.code {
                                KeyCode::Char(c) => query.push(c),
                                KeyCode::Enter => break,
                                KeyCode::Backspace => {
                                    query.pop();
                                }
                                _ => {}
                            }
                        }
                        terminal.draw(|f| search_ui(f, &query))?;
                    }
                    params.set_query(query);
                    let items = get_items(params).await?;
                    app.update_items(items);
                    terminal.draw(|f| ui(f, &mut app))?;
                }
                KeyCode::Char('o') => {
                    open_url(&format!(
                        "https://nyaa.si/view/{}",
                        app.items[app.current.unwrap_or(0)].id
                    ));
                }
                KeyCode::Char('m') => {
                    open_url(&app.items[app.current.unwrap_or(0)].magnet.to_string());
                }
                KeyCode::Char('t') => {
                    open_url(&app.items[app.current.unwrap_or(0)].torrent.to_string());
                }
                KeyCode::Char('b') => {
                    params.set_query("");
                    let items = get_items(params).await?;
                    app.update_items(items);
                    terminal.draw(|f| ui(f, &mut app))?;
                }
                KeyCode::Char('s') => {
                    let id = app.items[app.current.unwrap_or(0)]
                        .id
                        .parse::<u64>()
                        .unwrap_or(0);
                    app.set_id(id);
                }
                _ => {}
            }
        }
    }
}

fn search_ui<B: Backend>(f: &mut Frame<B>, text: &str) {
    let size = f.size();

    let chunks = Layout::default()
        .constraints([Constraint::Percentage(20)].as_ref())
        .split(size);

    let paragraph = Paragraph::new(Span::styled(text, Style::default()))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    f.render_widget(paragraph, chunks[0]);
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let rects = Layout::default()
        .constraints([Constraint::Percentage(100)].as_ref())
        .margin(1)
        .split(f.size());

    let selected_style = Style::default().add_modifier(Modifier::REVERSED);
    let normal_style = Style::default().bg(Color::Blue);
    let header_cells = ["Viewed", "Name", "Date", "Size", "Seeders", "Leechers"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Red)));
    let header = Row::new(header_cells)
        .style(normal_style)
        .height(1)
        .bottom_margin(1);
    let rows = app.items.iter().map(|item| {
        let Response {
            id,
            date,
            name,
            filesize,
            seeders,
            leechers,
            ..
        } = item;
        let height = 3;
        let viewed = if id.parse::<u64>().unwrap() <= app.last_id {
            "✅"
        } else {
            "❌"
        };
        let cells = [viewed, name, date, filesize, seeders, leechers]
            .map(|x| x.to_string())
            .map(Cell::from);
        Row::new(cells).height(height as u16).bottom_margin(1)
    });
    let t = Table::new(rows)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Table"))
        .highlight_style(selected_style)
        .highlight_symbol(">> ")
        .widths(&[
            Constraint::Min(8),
            Constraint::Min(60),
            Constraint::Min(10),
            Constraint::Min(10),
            Constraint::Min(7),
            Constraint::Min(9),
        ]);
    f.render_stateful_widget(t, rects[0], &mut app.state);
}
