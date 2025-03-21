use clap::Parser;
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
    widgets::{Block, Borders, Cell, Row, Table},
    Frame,
    Terminal,
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
    cursor_position: usize,          // Current cursor position
    scroll_offset: usize,            // Scroll offset for viewing expirations
    show_greeks: bool,               // Toggle to show/hide greeks
}

impl App {
    fn new(options_chain: OptionsChain) -> Self {
        let expiration_count = options_chain.expirations.len();
        App {
            options_chain,
            expanded_expirations: vec![false; expiration_count], // Start with all collapsed
            cursor_position: 0,
            scroll_offset: 0,
            show_greeks: true,
        }
    }

    fn toggle_current_expiration(&mut self) {
        if self.cursor_position < self.expanded_expirations.len() {
            self.expanded_expirations[self.cursor_position] =
                !self.expanded_expirations[self.cursor_position];
        }
    }

    fn toggle_greeks(&mut self) {
        self.show_greeks = !self.show_greeks;
    }

    fn move_cursor_down(&mut self) {
        if !self.expanded_expirations.is_empty() {
            self.cursor_position = (self.cursor_position + 1) % self.expanded_expirations.len();
            self.adjust_scroll();
        }
    }

    fn move_cursor_up(&mut self) {
        if !self.expanded_expirations.is_empty() {
            self.cursor_position = (self.cursor_position + self.expanded_expirations.len() - 1)
                % self.expanded_expirations.len();
            self.adjust_scroll();
        }
    }

    fn page_down(&mut self) {
        if !self.expanded_expirations.is_empty() {
            // Move cursor down by 5 positions (or to the end)
            let new_pos = std::cmp::min(
                self.cursor_position + 5,
                self.expanded_expirations.len() - 1,
            );
            self.cursor_position = new_pos;
            self.adjust_scroll();
        }
    }

    fn page_up(&mut self) {
        if !self.expanded_expirations.is_empty() {
            // Move cursor up by 5 positions (or to the beginning)
            self.cursor_position = self.cursor_position.saturating_sub(5);
            self.adjust_scroll();
        }
    }

    // Adjust scroll offset to keep cursor visible
    fn adjust_scroll(&mut self) {
        // Keep cursor within visible area (assuming ~10 visible items)
        const VISIBLE_ITEMS: usize = 10;

        if self.cursor_position < self.scroll_offset {
            self.scroll_offset = self.cursor_position;
        } else if self.cursor_position >= self.scroll_offset + VISIBLE_ITEMS {
            self.scroll_offset = self.cursor_position - VISIBLE_ITEMS + 1;
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

/// Command line arguments
#[derive(Parser, Debug)]
#[clap(author, version, about = "Options chain viewer")]
struct Args {
    /// Path to the options chain JSON file
    #[clap(default_value = "sample-options-chain.json")]
    filename: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Parse command line arguments
    let args = Args::parse();

    // Read the options chain from the specified JSON file
    let options_chain = read_options_chain(&args.filename)?;

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
                KeyCode::Down => app.move_cursor_down(),
                KeyCode::Up => app.move_cursor_up(),
                KeyCode::PageDown => app.page_down(),
                KeyCode::PageUp => app.page_up(),
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

    // Create main layout with just a title and content area
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(size);

    // Render title block
    let title_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "{} - ${:.2} - {} - Use ↑/↓/PgUp/PgDn to navigate, 'e' to expand/collapse, 'g' to toggle Greeks",
            app.options_chain.symbol, app.options_chain.last_price, app.options_chain.last_update
        ));
    f.render_widget(title_block, chunks[0]);

    // Render all expirations in the main area
    render_expirations_list(f, app, chunks[1]);
}

fn render_expirations_list<B: Backend>(f: &mut Frame<B>, app: &App, area: Rect) {
    let expirations = &app.options_chain.expirations;

    // Determine visible range based on scroll offset
    let visible_start = app.scroll_offset;
    let visible_end = std::cmp::min(expirations.len(), visible_start + (area.height as usize));

    // Calculate heights for visible expirations
    let mut visible_expirations = Vec::new();
    let mut constraints = Vec::new();
    let mut total_min_height = 0;

    for i in visible_start..visible_end {
        visible_expirations.push(i);

        // Calculate minimum height for this expiration
        let mut height = 3; // Header + border

        if app.expanded_expirations[i] {
            // Add space for options table
            height += expirations[i].options.len() as u16 + 2; // +2 for table header and padding
        }

        total_min_height += height;
        constraints.push(Constraint::Min(height));
    }

    // If we have space left, make the last constraint take the remaining space
    if !constraints.is_empty() && total_min_height < area.height {
        let last_idx = constraints.len() - 1;
        if let Constraint::Min(min_height) = constraints[last_idx] {
            constraints[last_idx] = Constraint::Min(min_height + (area.height - total_min_height));
        }
    }

    // Create layout for visible expirations
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    // Render visible expirations
    for (chunk_idx, &exp_idx) in visible_expirations.iter().enumerate() {
        let expiration = &expirations[exp_idx];
        let expanded = app.expanded_expirations[exp_idx];
        let prefix = if expanded { "[-] " } else { "[+] " };

        // Style based on cursor position
        let style = if exp_idx == app.cursor_position {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };

        // Create the expiration header
        let header = Spans::from(vec![Span::styled(
            format!("{}{}", prefix, expiration.date),
            style,
        )]);

        let border_style = if exp_idx == app.cursor_position {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };

        let expiration_block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(header);

        f.render_widget(expiration_block, chunks[chunk_idx]);

        // If expanded, render the options table inside
        if expanded {
            let inner_area = Rect {
                x: chunks[chunk_idx].x + 1,
                y: chunks[chunk_idx].y + 1,
                width: chunks[chunk_idx].width - 2,
                height: chunks[chunk_idx].height - 2,
            };

            render_options_table(f, app, inner_area, exp_idx);
        }
    }

    // Show scroll indicators if needed
    if visible_start > 0 || visible_end < expirations.len() {
        let scroll_text = format!("Scroll: {}/{}", app.cursor_position + 1, expirations.len());
        let scroll_text_len = scroll_text.len();
        let scroll_indicator = Spans::from(vec![Span::styled(
            scroll_text,
            Style::default().fg(Color::White),
        )]);

        let scroll_area = Rect {
            x: area.x + area.width - scroll_text_len as u16 - 2,
            y: area.y,
            width: scroll_text_len as u16,
            height: 1,
        };

        f.render_widget(tui::widgets::Paragraph::new(scroll_indicator), scroll_area);
    }
}

fn render_options_table<B: Backend>(
    f: &mut Frame<B>,
    app: &App,
    area: Rect,
    expiration_idx: usize,
) {
    let current_expiration = &app.options_chain.expirations[expiration_idx];

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
    let table = Table::new(rows)
        .header(header)
        .block(Block::default())
        .widths(&constraints)
        .column_spacing(1);

    f.render_widget(table, area);
}
