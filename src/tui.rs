//! Interactive terminal UI: a reusable master/detail browser.
//!
//! Commands that produce a list of things (prompts, flow events, journey
//! stages, prompt-history entries) build a [`Browser`] and hand it to [`run`].
//! The user navigates the list with the arrow keys; the right pane shows the
//! selected item's detail, and <kbd>Enter</kbd> expands that detail to a
//! scrollable full-screen view. Pressing <kbd>q</kbd>/<kbd>Esc</kbd> backs out.
//!
//! This is only ever reached when stdout is an interactive terminal and the
//! output format is text (see [`Ctx::interactive`]); JSON/DOT and piped output
//! keep the plain renderers.
//!
//! [`Ctx::interactive`]: crate::commands::Ctx

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::error::Result;

/// Visual emphasis for a list entry, mirroring the plain renderers' colors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ItemStyle {
    /// No special color (default foreground).
    #[default]
    Plain,
    /// Positive / committed (green).
    Good,
    /// Pending / attention (red).
    Bad,
    /// Secondary highlight (cyan).
    Accent,
}

impl ItemStyle {
    fn color(self) -> Option<Color> {
        match self {
            ItemStyle::Plain => None,
            ItemStyle::Good => Some(Color::Green),
            ItemStyle::Bad => Some(Color::Red),
            ItemStyle::Accent => Some(Color::Cyan),
        }
    }
}

/// One selectable row plus the detail shown when it is selected.
#[derive(Debug, Clone)]
pub struct Item {
    /// Single-line label shown in the list pane.
    pub label: String,
    /// Color emphasis for the label.
    pub style: ItemStyle,
    /// Multi-line text shown in the detail pane.
    pub detail: String,
}

impl Item {
    /// Convenience constructor for a plain (uncolored) item.
    pub fn new(label: impl Into<String>, detail: impl Into<String>) -> Self {
        Item {
            label: label.into(),
            style: ItemStyle::Plain,
            detail: detail.into(),
        }
    }

    /// Builder: set the label color.
    pub fn styled(mut self, style: ItemStyle) -> Self {
        self.style = style;
        self
    }
}

/// A navigable list with a per-item detail pane.
#[derive(Debug, Clone)]
pub struct Browser {
    /// Title shown on the list pane border.
    pub title: String,
    /// The rows to browse (must be non-empty for [`run`] to be interactive).
    pub items: Vec<Item>,
}

impl Browser {
    /// Build a browser with the given list title and items.
    pub fn new(title: impl Into<String>, items: Vec<Item>) -> Self {
        Browser {
            title: title.into(),
            items,
        }
    }
}

/// Mutable UI state for one browsing session.
struct State {
    selected: usize,
    /// Vertical scroll offset within the detail pane.
    detail_scroll: u16,
    /// Whether the detail pane is expanded full-screen and focused.
    expanded: bool,
}

/// Run the interactive browser, taking over the terminal until the user quits.
///
/// If `items` is empty there is nothing to browse, so this prints the title and
/// returns immediately rather than entering raw mode.
pub fn run(browser: &Browser) -> Result<()> {
    if browser.items.is_empty() {
        println!("{}", browser.title);
        return Ok(());
    }

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, browser);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, browser: &Browser) -> Result<()> {
    let mut state = State {
        selected: 0,
        detail_scroll: 0,
        expanded: false,
    };
    let last = browser.items.len() - 1;

    loop {
        terminal.draw(|frame| draw(frame, browser, &mut state))?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if state.expanded {
                    state.expanded = false;
                    state.detail_scroll = 0;
                } else {
                    break;
                }
            }
            KeyCode::Enter => {
                state.expanded = true;
                state.detail_scroll = 0;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.expanded {
                    state.detail_scroll = state.detail_scroll.saturating_add(1);
                } else if state.selected < last {
                    state.selected += 1;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if state.expanded {
                    state.detail_scroll = state.detail_scroll.saturating_sub(1);
                } else {
                    state.selected = state.selected.saturating_sub(1);
                }
            }
            KeyCode::PageDown => {
                if state.expanded {
                    state.detail_scroll = state.detail_scroll.saturating_add(10);
                } else {
                    state.selected = (state.selected + 10).min(last);
                }
            }
            KeyCode::PageUp => {
                if state.expanded {
                    state.detail_scroll = state.detail_scroll.saturating_sub(10);
                } else {
                    state.selected = state.selected.saturating_sub(10);
                }
            }
            KeyCode::Home | KeyCode::Char('g') => {
                if state.expanded {
                    state.detail_scroll = 0;
                } else {
                    state.selected = 0;
                }
            }
            KeyCode::End | KeyCode::Char('G') if !state.expanded => {
                state.selected = last;
            }
            _ => {}
        }
    }
    Ok(())
}

fn draw(frame: &mut Frame, browser: &Browser, state: &mut State) {
    let [body, help] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

    if state.expanded {
        draw_detail(frame, browser, state, body, true);
    } else {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)])
                .areas(body);
        draw_list(frame, browser, state, left);
        draw_detail(frame, browser, state, right, false);
    }

    let hint = if state.expanded {
        " ↑/↓ scroll   q/Esc back"
    } else {
        " ↑/↓ move   ⏎ open   q quit"
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        help,
    );
}

fn draw_list(frame: &mut Frame, browser: &Browser, state: &mut State, area: Rect) {
    let items: Vec<ListItem> = browser
        .items
        .iter()
        .map(|it| {
            let mut style = Style::default();
            if let Some(c) = it.style.color() {
                style = style.fg(c);
            }
            ListItem::new(Line::from(Span::styled(it.label.clone(), style)))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", browser.title)),
        )
        .highlight_symbol("› ")
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn draw_detail(frame: &mut Frame, browser: &Browser, state: &State, area: Rect, expanded: bool) {
    let item = &browser.items[state.selected];
    let title = if expanded {
        format!(" {} ", item.label.trim())
    } else {
        " Detail ".to_string()
    };
    let paragraph = Paragraph::new(item.detail.as_str())
        .block(Block::default().borders(Borders::ALL).title(title))
        .wrap(Wrap { trim: false })
        .scroll((state.detail_scroll, 0));
    frame.render_widget(paragraph, area);
}
