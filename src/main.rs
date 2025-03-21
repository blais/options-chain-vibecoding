use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fs::File,
    io::{self, BufReader},
    path::Path,
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    // symbols,
    text::{Span, Spans},
    widgets::{Block, Borders, Cell, Row, Table, Tabs},
    Frame, Terminal,
};

// Define the structs that match our JSON schema
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Greeks {
    delta: f64,
    gamma: f64,
    theta: f64,
    vega: f64,
    rho: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct OptionData {
    symbol: String,
    bid: f64,
    ask: f64,
    #[serde(rename = "bidSize")]
    bid_size: i64,
    #[serde(rename = "askSize")]
    ask_size: i64,
    volume: i64,
    #[serde(rename = "openInterest")]
    open_interest: i64,
    greeks: Greeks,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct OptionPair {
    strike: f64,
    call: OptionData,
    put: OptionData,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct Expiration {
    date: String,
    options: Vec<OptionPair>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct OptionsChain {
    symbol: String,
    #[serde(rename = "lastPrice")]
    last_price: f64,
    #[serde(rename = "lastUpdate")]
    last_update: String,
    expirations: Vec<Expiration>,
}

// App state
struct App {
    options_chain: OptionsChain,
    expanded_expirations: Vec<bool>, // Track which expirations are expanded
    current_tab: usize,              // Currently selected expiration
    show_greeks: bool,               // Toggle to show/hide greeks
}

impl App {
    fn new(options_chain: OptionsChain) -> Self {
        let expiration_count = options_chain.expirations.len();
        App {
            options_chain,
            expanded_expirations: vec![true; expiration_count], // Start with all expanded
            current_tab: 0,
            show_greeks: true,
        }
    }

    fn toggle_current_expiration(&mut self) {
        if self.current_tab < self.expanded_expirations.len() {
            self.expanded_expirations[self.current_tab] =
                !self.expanded_expirations[self.current_tab];
        }
    }

    fn toggle_greeks(&mut self) {
        self.show_greeks = !self.show_greeks;
    }

    fn next_tab(&mut self) {
        if !self.expanded_expirations.is_empty() {
            self.current_tab = (self.current_tab + 1) % self.expanded_expirations.len();
        }
    }

    fn prev_tab(&mut self) {
        if !self.expanded_expirations.is_empty() {
            self.current_tab = (self.current_tab + self.expanded_expirations.len() - 1)
                % self.expanded_expirations.len();
        }
    }
}

fn read_options_chain<P: AsRef<Path>>(path: P) -> Result<OptionsChain, Box<dyn Error>> {
    // Open the file
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    // Parse the JSON file
    let options_chain: OptionsChain = serde_json::from_reader(reader)?;
    Ok(options_chain)
}

fn main() -> Result<(), Box<dyn Error>> {
    // Read the options chain from a JSON file
    // You can change the path to your JSON file
    let options_chain = read_options_chain("options_chain.json")?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new(options_chain);

    // Main loop
    loop {
        terminal.draw(|f| ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('e') | KeyCode::Enter => app.toggle_current_expiration(),
                KeyCode::Char('g') => app.toggle_greeks(),
                KeyCode::Right | KeyCode::Tab => app.next_tab(),
                KeyCode::Left | KeyCode::BackTab => app.prev_tab(),
                _ => {}
            }
        }
    }

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &App) {
    let size = f.size();

    // Create top level layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(size);

    // Create the expiration date tabs
    let expiration_tabs = app
        .options_chain
        .expirations
        .iter()
        .enumerate()
        .map(|(i, exp)| {
            let expanded = app.expanded_expirations[i];
            let prefix = if expanded { "[-] " } else { "[+] " };
            Spans::from(vec![Span::styled(
                format!("{}{}", prefix, exp.date),
                Style::default().fg(Color::Green),
            )])
        })
        .collect();

    let tabs = Tabs::new(expiration_tabs)
        .block(Block::default().borders(Borders::ALL).title(format!(
            "{} - ${:.2} - {}",
            app.options_chain.symbol, app.options_chain.last_price, app.options_chain.last_update
        )))
        .select(app.current_tab)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs, chunks[0]);

    // If the current expiration is expanded, show its options
    if app.current_tab < app.options_chain.expirations.len()
        && app.expanded_expirations[app.current_tab]
    {
        let options_area = chunks[1];
        render_options_table(f, app, options_area);
    } else {
        // Render a hint if current expiration is collapsed
        let block = Block::default()
            .title("Press 'e' to expand")
            .borders(Borders::ALL);
        f.render_widget(block, chunks[1]);
    }
}

fn render_options_table<B: Backend>(f: &mut Frame<B>, app: &App, area: Rect) {
    let current_expiration = &app.options_chain.expirations[app.current_tab];

    // Define table widths based on whether we're showing greeks
    let mut constraints = vec![
        Constraint::Length(10), // Call Symbol
        Constraint::Length(8),  // Bid
        Constraint::Length(8),  // Ask
        Constraint::Length(8),  // Bid Size
        Constraint::Length(8),  // Ask Size
        Constraint::Length(8),  // Volume
    ];

    if app.show_greeks {
        constraints.extend(vec![
            Constraint::Length(8), // Delta
            Constraint::Length(8), // Gamma
            Constraint::Length(8), // Vega
        ]);
    }

    constraints.extend(vec![
        Constraint::Length(10), // Strike
        Constraint::Length(10), // Put Symbol
        Constraint::Length(8),  // Bid
        Constraint::Length(8),  // Ask
        Constraint::Length(8),  // Bid Size
        Constraint::Length(8),  // Ask Size
        Constraint::Length(8),  // Volume
    ]);

    if app.show_greeks {
        constraints.extend(vec![
            Constraint::Length(8), // Delta
            Constraint::Length(8), // Gamma
            Constraint::Length(8), // Vega
        ]);
    }

    // Create header row
    let mut header_cells = vec![
        Cell::from(Span::styled("Call Sym", Style::default().fg(Color::Cyan))),
        Cell::from(Span::styled("Bid", Style::default().fg(Color::Green))),
        Cell::from(Span::styled("Ask", Style::default().fg(Color::Red))),
        Cell::from(Span::styled("Bid Size", Style::default().fg(Color::Green))),
        Cell::from(Span::styled("Ask Size", Style::default().fg(Color::Red))),
        Cell::from(Span::styled("Volume", Style::default().fg(Color::Yellow))),
    ];

    if app.show_greeks {
        header_cells.extend(vec![
            Cell::from(Span::styled("Delta", Style::default().fg(Color::Magenta))),
            Cell::from(Span::styled("Gamma", Style::default().fg(Color::Magenta))),
            Cell::from(Span::styled("Vega", Style::default().fg(Color::Magenta))),
        ]);
    }

    header_cells.extend(vec![
        Cell::from(Span::styled(
            "Strike",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Cell::from(Span::styled("Put Sym", Style::default().fg(Color::Cyan))),
        Cell::from(Span::styled("Bid", Style::default().fg(Color::Green))),
        Cell::from(Span::styled("Ask", Style::default().fg(Color::Red))),
        Cell::from(Span::styled("Bid Size", Style::default().fg(Color::Green))),
        Cell::from(Span::styled("Ask Size", Style::default().fg(Color::Red))),
        Cell::from(Span::styled("Volume", Style::default().fg(Color::Yellow))),
    ]);

    if app.show_greeks {
        header_cells.extend(vec![
            Cell::from(Span::styled("Delta", Style::default().fg(Color::Magenta))),
            Cell::from(Span::styled("Gamma", Style::default().fg(Color::Magenta))),
            Cell::from(Span::styled("Vega", Style::default().fg(Color::Magenta))),
        ]);
    }

    let header = Row::new(header_cells)
        .style(Style::default().fg(Color::White))
        .height(1);

    // Create option rows
    let rows = current_expiration
        .options
        .iter()
        .map(|option| {
            let mut cells = vec![
                Cell::from(option.call.symbol.clone()),
                Cell::from(format!("{:.2}", option.call.bid)),
                Cell::from(format!("{:.2}", option.call.ask)),
                Cell::from(option.call.bid_size.to_string()),
                Cell::from(option.call.ask_size.to_string()),
                Cell::from(option.call.volume.to_string()),
            ];

            if app.show_greeks {
                cells.extend(vec![
                    Cell::from(format!("{:.4}", option.call.greeks.delta)),
                    Cell::from(format!("{:.4}", option.call.greeks.gamma)),
                    Cell::from(format!("{:.4}", option.call.greeks.vega)),
                ]);
            }

            // Calculate strike color based on relation to current stock price
            let strike_color = if option.strike < app.options_chain.last_price {
                Color::Green
            } else if option.strike > app.options_chain.last_price {
                Color::Red
            } else {
                Color::Yellow
            };

            cells.push(Cell::from(Span::styled(
                format!("{:.2}", option.strike),
                Style::default()
                    .fg(strike_color)
                    .add_modifier(Modifier::BOLD),
            )));

            cells.extend(vec![
                Cell::from(option.put.symbol.clone()),
                Cell::from(format!("{:.2}", option.put.bid)),
                Cell::from(format!("{:.2}", option.put.ask)),
                Cell::from(option.put.bid_size.to_string()),
                Cell::from(option.put.ask_size.to_string()),
                Cell::from(option.put.volume.to_string()),
            ]);

            if app.show_greeks {
                cells.extend(vec![
                    Cell::from(format!("{:.4}", option.put.greeks.delta)),
                    Cell::from(format!("{:.4}", option.put.greeks.gamma)),
                    Cell::from(format!("{:.4}", option.put.greeks.vega)),
                ]);
            }

            Row::new(cells)
                .height(1)
                .style(Style::default().fg(Color::White))
        })
        .collect::<Vec<_>>();

    // Create the table widget
    let title = format!(
        "Options Chain - {} - Toggle Greeks: 'g', Expand/Collapse: 'e'",
        current_expiration.date
    );
    let table = Table::new(rows)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .widths(&constraints)
        .column_spacing(1);

    f.render_widget(table, area);
}
