use crossterm::event::{self, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use forja_core::error::{ForjaError, Result};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use rusqlite::{Connection, OpenFlags};
use std::io::{self, Stdout};
use std::path::Path;
use std::time::Duration;

pub fn run_tui_view(audit_db: &Path, memory_db: &Path) -> Result<()> {
    enable_raw_mode().map_err(|error| ForjaError::Internal(error.to_string()))?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)
        .map_err(|error| ForjaError::Internal(error.to_string()))?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal =
        Terminal::new(backend).map_err(|error| ForjaError::Internal(error.to_string()))?;
    let result = run_loop(&mut terminal, audit_db, memory_db);

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    audit_db: &Path,
    memory_db: &Path,
) -> Result<()> {
    loop {
        let history = load_recent_history(audit_db)?;
        let tool_state = load_tool_state(audit_db)?;
        let memory_state = load_memory_state(memory_db)?;

        terminal
            .draw(|frame| {
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(3),
                        Constraint::Min(10),
                        Constraint::Length(8),
                    ])
                    .split(frame.area());

                let header = Paragraph::new(vec![Line::from(vec![
                    Span::styled("Forja TUI", Style::default().fg(Color::Cyan)),
                    Span::raw("  "),
                    Span::raw("q: quit, r: refresh"),
                ])])
                .block(Block::default().borders(Borders::ALL).title("Status"));
                frame.render_widget(header, chunks[0]);

                let middle = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                    .split(chunks[1]);

                let history_widget = Paragraph::new(history)
                    .block(Block::default().borders(Borders::ALL).title("History"))
                    .wrap(Wrap { trim: true });
                frame.render_widget(history_widget, middle[0]);

                let tool_widget = Paragraph::new(tool_state)
                    .block(Block::default().borders(Borders::ALL).title("Tool Status"))
                    .wrap(Wrap { trim: true });
                frame.render_widget(tool_widget, middle[1]);

                let memory_widget = Paragraph::new(memory_state)
                    .block(Block::default().borders(Borders::ALL).title("Memory State"))
                    .wrap(Wrap { trim: true });
                frame.render_widget(memory_widget, chunks[2]);
            })
            .map_err(|error| ForjaError::Internal(error.to_string()))?;

        if event::poll(Duration::from_millis(250))
            .map_err(|error| ForjaError::Internal(error.to_string()))?
            && let Event::Key(key) =
                event::read().map_err(|error| ForjaError::Internal(error.to_string()))?
        {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Char('r') => {}
                _ => {}
            }
        }
    }

    Ok(())
}

fn load_recent_history(audit_db: &Path) -> Result<String> {
    let connection = open_read_only(audit_db)?;
    let mut statement = connection
        .prepare(
            "SELECT timestamp, event_type, payload
             FROM audit_log
             ORDER BY id DESC
             LIMIT 20",
        )
        .map_err(|error| ForjaError::Storage(error.to_string()))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| ForjaError::Storage(error.to_string()))?;

    let mut lines = Vec::new();
    for row in rows {
        let (timestamp, event_type, payload) =
            row.map_err(|error| ForjaError::Storage(error.to_string()))?;
        lines.push(format!("{timestamp} | {event_type} | {payload}"));
    }

    if lines.is_empty() {
        Ok("No recent history.".to_string())
    } else {
        Ok(lines.join("\n"))
    }
}

fn load_tool_state(audit_db: &Path) -> Result<String> {
    let connection = open_read_only(audit_db)?;
    let mut statement = connection
        .prepare(
            "SELECT event_type, payload
             FROM audit_log
             WHERE event_type IN ('tool_call', 'tool_result', 'llm_call', 'compression')
             ORDER BY id DESC
             LIMIT 12",
        )
        .map_err(|error| ForjaError::Storage(error.to_string()))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| ForjaError::Storage(error.to_string()))?;

    let mut lines = Vec::new();
    for row in rows {
        let (event_type, payload) = row.map_err(|error| ForjaError::Storage(error.to_string()))?;
        lines.push(format!("{event_type}: {payload}"));
    }

    if lines.is_empty() {
        Ok("No tool activity.".to_string())
    } else {
        Ok(lines.join("\n"))
    }
}

fn load_memory_state(memory_db: &Path) -> Result<String> {
    if !memory_db.exists() {
        return Ok("memory.db not found.".to_string());
    }

    let connection = Connection::open_with_flags(memory_db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| ForjaError::Storage(error.to_string()))?;
    let entries = query_count(&connection, "SELECT COUNT(*) FROM memory_entries")?;
    let summaries = query_count(&connection, "SELECT COUNT(*) FROM memory_summaries")?;
    Ok(format!(
        "memory_entries={entries}\nmemory_summaries={summaries}"
    ))
}

fn query_count(connection: &Connection, sql: &str) -> Result<i64> {
    connection
        .query_row(sql, [], |row| row.get::<_, i64>(0))
        .map_err(|error| ForjaError::Storage(error.to_string()))
}

fn open_read_only(path: &Path) -> Result<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| ForjaError::Storage(error.to_string()))
}
