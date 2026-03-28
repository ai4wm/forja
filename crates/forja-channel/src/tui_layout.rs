#[cfg(feature = "tui")]
use crate::tui_channel::{DisplayBuffer, FocusPane};
#[cfg(feature = "tui")]
use ratatui::Frame;
#[cfg(feature = "tui")]
use ratatui::layout::{Constraint, Direction, Layout};
#[cfg(feature = "tui")]
use ratatui::style::{Color, Style};
#[cfg(feature = "tui")]
use ratatui::text::{Line, Span};
#[cfg(feature = "tui")]
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
#[cfg(feature = "tui")]
use std::collections::VecDeque;
#[cfg(feature = "tui")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "tui")]
use crate::tui_channel::{AgentStatusInfo, TuiState};
#[cfg(feature = "tui")]
use forja_core::notification::{Notification, NotificationLevel};

#[cfg(feature = "tui")]
pub(crate) fn render(frame: &mut Frame, state: &Arc<Mutex<TuiState>>) {
    let Ok(state) = state.lock() else {
        return;
    };
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(frame.area());
    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(layout[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main[1]);

    render_conversation(frame, main[0], &state.messages, state.focus == FocusPane::Conversation);
    render_status(frame, right[0], &state.agent_status);
    render_notifications(
        frame,
        right[1],
        &state.notifications,
        state.focus == FocusPane::Notifications,
    );
    render_input(frame, layout[1], &state.input, state.focus == FocusPane::Input);

    if state.help_overlay {
        render_help(frame);
    }

    if let Some(confirm) = &state.pending_confirm {
        render_confirm(frame, &confirm.prompt);
    }
}

#[cfg(feature = "tui")]
fn render_conversation(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    buffer: &Arc<Mutex<DisplayBuffer>>,
    focused: bool,
) {
    let items = buffer
        .lock()
        .map(|buffer| {
            buffer
                .messages
                .iter()
                .map(|message| {
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("{}: ", message.role),
                            Style::default().fg(Color::Cyan),
                        ),
                        Span::raw(message.text.clone()),
                    ]))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let block = Block::default()
        .title("Conversation")
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        });
    frame.render_widget(List::new(items).block(block), area);
}

#[cfg(feature = "tui")]
fn render_status(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    status: &Arc<Mutex<AgentStatusInfo>>,
) {
    let status = status.lock().map(|status| status.clone()).unwrap_or_default();
    let lines = vec![
        Line::from(format!("Mode: {}", status.exec_mode)),
        Line::from(format!("Model: {}", status.model_name)),
        Line::from(format!("Think: {}", status.think_level)),
        Line::from(format!("Role: {}", status.role)),
        Line::from(format!("Background: {}", status.background_running)),
        Line::from(format!("Paused: {}", status.background_paused)),
        Line::from(format!("Memory: {} entries", status.memory_entry_count)),
        Line::from(format!("Uptime: {}s", status.uptime_seconds)),
        Line::from(format!(
            "Last event: {}",
            status.last_event.unwrap_or_else(|| "none".to_string())
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title("Agent Status").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

#[cfg(feature = "tui")]
fn render_notifications(
    frame: &mut Frame,
    area: ratatui::layout::Rect,
    notifications: &Arc<Mutex<VecDeque<Notification>>>,
    focused: bool,
) {
    let items = notifications
        .lock()
        .map(|notifications| {
            notifications
                .iter()
                .rev()
                .take(10)
                .map(|notification| {
                    let color = match notification.level {
                        NotificationLevel::Info => Color::White,
                        NotificationLevel::Warning => Color::Yellow,
                        NotificationLevel::Critical => Color::Red,
                    };
                    ListItem::new(Line::from(Span::styled(
                        format!("{} - {}", notification.title, notification.body),
                        Style::default().fg(color),
                    )))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let block = Block::default()
        .title("Notifications")
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        });
    frame.render_widget(List::new(items).block(block), area);
}

#[cfg(feature = "tui")]
fn render_input(frame: &mut Frame, area: ratatui::layout::Rect, input: &str, focused: bool) {
    let block = Block::default()
        .title("Input")
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default()
        });
    frame.render_widget(
        Paragraph::new(format!("{input}    [Ctrl+Q] exit  [F1] help"))
            .block(block)
            .wrap(Wrap { trim: true }),
        area,
    );
}

#[cfg(feature = "tui")]
fn render_help(frame: &mut Frame) {
    let area = centered_rect(60, 60, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(
            "TUI help\n\nEnter: send message\nCtrl+Q or Esc: exit TUI\nUp/Down: scroll conversation\nTab: cycle focus\nCtrl+L: clear conversation display\nF1: toggle this help",
        )
        .block(Block::default().title("Help").borders(Borders::ALL))
        .wrap(Wrap { trim: true }),
        area,
    );
}

#[cfg(feature = "tui")]
fn render_confirm(frame: &mut Frame, prompt: &str) {
    let area = centered_rect(60, 20, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(format!("{prompt}\n\n[Y/n]"))
            .block(Block::default().title("Confirm").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

#[cfg(feature = "tui")]
fn centered_rect(percent_x: u16, percent_y: u16, area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
