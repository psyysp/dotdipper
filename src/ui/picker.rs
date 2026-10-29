//! A full-screen checklist for choosing many items at once.
//!
//! It takes over the terminal's alternate screen, so a list longer than the
//! window scrolls inside it instead of breaking the redraw, and it reads keys
//! from the terminal itself, so it works under `curl ... | sh` as well.
//!
//! Keys work the same in both styles: `j`/`k` or the arrows move, space
//! toggles, `/` filters, enter confirms. The key handling lives in
//! [`Picker`], apart from the terminal, so it is unit tested.

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{
    Attribute, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::{cursor, execute, queue, terminal};
use std::io::{IsTerminal, Write};

/// One row of the checklist.
#[derive(Debug, Clone)]
pub struct PickItem {
    /// A short, colored group label in front of the row, such as "App".
    pub tag: Option<(String, Color)>,
    pub label: String,
    /// Shown dimmed after the label: what the item is.
    pub detail: Option<String>,
    /// Shown last, in red: why this row deserves a second look.
    pub warning: Option<String>,
    pub checked: bool,
}

pub use crossterm::style::Color;

impl PickItem {
    pub fn new(label: impl Into<String>, checked: bool) -> Self {
        Self {
            tag: None,
            label: label.into(),
            detail: None,
            warning: None,
            checked,
        }
    }

    pub fn with_tag(mut self, tag: impl Into<String>, color: Color) -> Self {
        self.tag = Some((tag.into(), color));
        self
    }

    pub fn with_detail(mut self, detail: Option<String>) -> Self {
        self.detail = detail;
        self
    }

    pub fn with_warning(mut self, warning: Option<String>) -> Self {
        self.warning = warning;
        self
    }

    fn matches(&self, filter: &str) -> bool {
        if filter.is_empty() {
            return true;
        }
        let filter = filter.to_lowercase();
        [
            self.tag.as_ref().map(|(t, _)| t.as_str()),
            Some(self.label.as_str()),
            self.detail.as_deref(),
            self.warning.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|text| text.to_lowercase().contains(&filter))
    }
}

/// Key, then what it does; the key is drawn highlighted.
pub const HELP: &[(&str, &str)] = &[
    ("j/k ↑/↓", "move"),
    ("space", "toggle"),
    ("a/n", "check/uncheck shown"),
    ("/", "filter"),
    ("g/G", "ends"),
    ("enter", "done"),
    ("q", "quit"),
];

// 256-color palette entries: in every common terminal theme, light and dark.
const CURSOR_BG: Color = Color::AnsiValue(237);
const MUTED: Color = Color::AnsiValue(245);

/// A run of text in one style.
struct Seg {
    text: String,
    fg: Option<Color>,
    bold: bool,
}

fn seg(text: impl Into<String>, fg: Option<Color>, bold: bool) -> Seg {
    Seg {
        text: text.into(),
        fg,
        bold,
    }
}

/// Prints `segs` on one line, cut to `width`, and with `bg` fills the rest
/// of the line so the highlight reads as a bar.
fn print_line(out: &mut impl Write, segs: &[Seg], width: usize, bg: Option<Color>) -> Result<()> {
    if let Some(bg) = bg {
        queue!(out, SetBackgroundColor(bg))?;
    }
    let mut room = width;
    for s in segs {
        if room == 0 {
            break;
        }
        let text = console::truncate_str(&s.text, room, "…");
        room = room.saturating_sub(console::measure_text_width(&text));
        if let Some(fg) = s.fg {
            queue!(out, SetForegroundColor(fg))?;
        }
        if s.bold {
            queue!(out, SetAttribute(Attribute::Bold))?;
        }
        queue!(out, Print(text), SetAttribute(Attribute::NormalIntensity))?;
        if s.fg.is_some() {
            queue!(out, SetForegroundColor(Color::Reset))?;
        }
    }
    if bg.is_some() {
        queue!(out, Print(" ".repeat(room)))?;
    }
    queue!(
        out,
        ResetColor,
        SetAttribute(Attribute::Reset),
        Print("\r\n")
    )?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Continue,
    Confirm,
    Cancel,
}

/// The checklist's state and key handling, with no terminal attached.
#[derive(Debug)]
pub struct Picker {
    items: Vec<PickItem>,
    filter: String,
    /// True while keys are being typed into the filter.
    filtering: bool,
    /// Position within [`Picker::visible`], not within `items`.
    cursor: usize,
    /// First visible row on screen, for scrolling.
    offset: usize,
    /// Rows the screen can show; set by the renderer.
    page: usize,
}

impl Picker {
    pub fn new(items: Vec<PickItem>) -> Self {
        Self {
            items,
            filter: String::new(),
            filtering: false,
            cursor: 0,
            offset: 0,
            page: 10,
        }
    }

    /// Indices into `items` that the filter shows, in order.
    pub fn visible(&self) -> Vec<usize> {
        (0..self.items.len())
            .filter(|&i| self.items[i].matches(&self.filter))
            .collect()
    }

    /// Indices of the checked items, in their original order.
    pub fn checked(&self) -> Vec<usize> {
        (0..self.items.len())
            .filter(|&i| self.items[i].checked)
            .collect()
    }

    fn set_page(&mut self, page: usize) {
        self.page = page.max(1);
        self.scroll_to_cursor();
    }

    fn scroll_to_cursor(&mut self) {
        let shown = self.visible().len();
        self.cursor = self.cursor.min(shown.saturating_sub(1));
        if self.cursor < self.offset {
            self.offset = self.cursor;
        } else if self.cursor >= self.offset + self.page {
            self.offset = self.cursor + 1 - self.page;
        }
        self.offset = self.offset.min(shown.saturating_sub(self.page));
    }

    fn move_by(&mut self, delta: isize) {
        let shown = self.visible().len();
        if shown == 0 {
            return;
        }
        let last = shown as isize - 1;
        self.cursor = (self.cursor as isize + delta).clamp(0, last) as usize;
        self.scroll_to_cursor();
    }

    fn set_shown(&mut self, checked: bool) {
        for i in self.visible() {
            self.items[i].checked = checked;
        }
    }

    fn toggle_current(&mut self) {
        if let Some(&i) = self.visible().get(self.cursor) {
            self.items[i].checked = !self.items[i].checked;
        }
    }

    fn filter_changed(&mut self) {
        self.cursor = 0;
        self.offset = 0;
    }

    pub fn handle(&mut self, key: KeyEvent) -> Outcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let half = (self.page / 2).max(1) as isize;

        // Keys that mean the same thing in either mode.
        match key.code {
            KeyCode::Char('c') if ctrl => return Outcome::Cancel,
            KeyCode::Up => {
                self.move_by(-1);
                return Outcome::Continue;
            }
            KeyCode::Down => {
                self.move_by(1);
                return Outcome::Continue;
            }
            KeyCode::PageUp => {
                self.move_by(-(self.page as isize));
                return Outcome::Continue;
            }
            KeyCode::PageDown => {
                self.move_by(self.page as isize);
                return Outcome::Continue;
            }
            KeyCode::Char('p') if ctrl => {
                self.move_by(-1);
                return Outcome::Continue;
            }
            KeyCode::Char('n') if ctrl => {
                self.move_by(1);
                return Outcome::Continue;
            }
            KeyCode::Char('u') if ctrl => {
                self.move_by(-half);
                return Outcome::Continue;
            }
            KeyCode::Char('d') if ctrl => {
                self.move_by(half);
                return Outcome::Continue;
            }
            _ => {}
        }

        if self.filtering {
            match key.code {
                // Enter keeps the filter and returns to the list.
                KeyCode::Enter => self.filtering = false,
                KeyCode::Esc => {
                    self.filtering = false;
                    self.filter.clear();
                    self.filter_changed();
                }
                KeyCode::Backspace => {
                    if self.filter.pop().is_none() {
                        self.filtering = false;
                    }
                    self.filter_changed();
                }
                KeyCode::Char(c) if !ctrl && !c.is_control() => {
                    self.filter.push(c);
                    self.filter_changed();
                }
                _ => {}
            }
            return Outcome::Continue;
        }

        match key.code {
            KeyCode::Enter => return Outcome::Confirm,
            KeyCode::Char('q') => return Outcome::Cancel,
            // Esc clears a filter; it never throws away the selection.
            KeyCode::Esc => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.filter_changed();
                }
            }
            KeyCode::Char('j') => self.move_by(1),
            KeyCode::Char('k') => self.move_by(-1),
            KeyCode::Char('g') | KeyCode::Home => self.move_by(-(self.items.len() as isize)),
            KeyCode::Char('G') | KeyCode::End => self.move_by(self.items.len() as isize),
            KeyCode::Char(' ') | KeyCode::Char('x') | KeyCode::Tab => self.toggle_current(),
            KeyCode::Char('a') => self.set_shown(true),
            KeyCode::Char('n') => self.set_shown(false),
            KeyCode::Char('/') => self.filtering = true,
            _ => {}
        }
        Outcome::Continue
    }

    fn render(&mut self, out: &mut impl Write, title: &str) -> Result<()> {
        let (cols, rows) = terminal::size().unwrap_or((80, 24));
        let width = cols as usize;
        // Title, status, blank, two scroll markers, blank, help, and the
        // help line's newline, which must not scroll the title away.
        self.set_page((rows as usize).saturating_sub(8));

        let visible = self.visible();
        queue!(
            out,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )?;
        print_line(out, &[seg(title, None, true)], width, None)?;

        let mut status = vec![
            seg(self.checked().len().to_string(), Some(Color::Green), true),
            seg(
                format!(" of {} checked", self.items.len()),
                Some(MUTED),
                false,
            ),
        ];
        if self.filtering || !self.filter.is_empty() {
            status.push(seg("   filter ", Some(MUTED), false));
            status.push(seg(
                format!("/{}{}", self.filter, if self.filtering { "▏" } else { "" }),
                Some(Color::Yellow),
                true,
            ));
            status.push(seg(
                format!("  {} shown", visible.len()),
                Some(MUTED),
                false,
            ));
        }
        print_line(out, &status, width, None)?;
        queue!(out, Print("\r\n"))?;

        let above = self.offset;
        let end = (self.offset + self.page).min(visible.len());
        let below = visible.len() - end;
        let marker = |n: usize, dir: &str| {
            let text = if n > 0 {
                format!("  {dir} {n} more")
            } else {
                String::new()
            };
            [seg(text, Some(MUTED), false)]
        };
        print_line(out, &marker(above, "↑"), width, None)?;
        if visible.is_empty() {
            print_line(
                out,
                &[seg(
                    "  nothing matches the filter",
                    Some(Color::Yellow),
                    false,
                )],
                width,
                None,
            )?;
        }
        for (row, &i) in visible[self.offset..end].iter().enumerate() {
            let item = &self.items[i];
            let here = self.offset + row == self.cursor;
            let mut segs = vec![
                if here {
                    seg("❯ ", Some(Color::Cyan), true)
                } else {
                    seg("  ", None, false)
                },
                if item.checked {
                    seg("[✓] ", Some(Color::Green), true)
                } else {
                    seg("[ ] ", Some(MUTED), false)
                },
            ];
            if let Some((tag, color)) = &item.tag {
                segs.push(seg(format!("{tag} "), Some(*color), true));
            }
            // An unchecked row is dimmed, so what will happen reads at a glance.
            let label_fg = if item.checked { None } else { Some(MUTED) };
            segs.push(seg(item.label.clone(), label_fg, item.checked || here));
            if let Some(detail) = &item.detail {
                segs.push(seg(format!("  {detail}"), Some(MUTED), false));
            }
            if let Some(warning) = &item.warning {
                segs.push(seg(format!("  [{warning}]"), Some(Color::Red), false));
            }
            print_line(out, &segs, width, here.then_some(CURSOR_BG))?;
        }
        print_line(out, &marker(below, "↓"), width, None)?;
        queue!(out, Print("\r\n"))?;

        let mut help = Vec::new();
        for (n, (key, what)) in HELP.iter().enumerate() {
            if n > 0 {
                help.push(seg(" · ", Some(MUTED), false));
            }
            help.push(seg(*key, Some(Color::Cyan), true));
            help.push(seg(format!(" {what}"), Some(MUTED), false));
        }
        print_line(out, &help, width, None)?;
        out.flush()?;
        Ok(())
    }
}

/// Puts the terminal back however the picker exits, including on a panic.
struct Screen;

impl Screen {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode().context("Could not take over the terminal")?;
        let screen = Screen;
        execute!(
            std::io::stderr(),
            terminal::EnterAlternateScreen,
            cursor::Hide
        )?;
        Ok(screen)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(
            std::io::stderr(),
            cursor::Show,
            terminal::LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}

/// True when a person can answer the picker: it draws on stderr and reads
/// keys from the terminal.
pub fn available() -> bool {
    std::io::stderr().is_terminal() && (cfg!(windows) || std::fs::File::open("/dev/tty").is_ok())
}

/// Shows the checklist and returns the indices of the checked items, or
/// `None` when the person cancels.
pub fn pick(title: &str, items: Vec<PickItem>) -> Result<Option<Vec<usize>>> {
    if !available() {
        anyhow::bail!("The checklist needs a terminal to run in");
    }
    let mut picker = Picker::new(items);
    let mut out = std::io::stderr();
    let _screen = Screen::enter()?;
    loop {
        picker.render(&mut out, title)?;
        match event::read().context("Could not read a key from the terminal")? {
            Event::Key(key) if key.kind != KeyEventKind::Release => match picker.handle(key) {
                Outcome::Continue => {}
                Outcome::Confirm => return Ok(Some(picker.checked())),
                Outcome::Cancel => return Ok(None),
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ch(c: char) -> KeyEvent {
        key(KeyCode::Char(c))
    }

    fn picker(n: usize) -> Picker {
        let mut p = Picker::new(
            (0..n)
                .map(|i| PickItem::new(format!("item-{i:02}"), true))
                .collect(),
        );
        p.set_page(5);
        p
    }

    fn press(p: &mut Picker, keys: &[KeyEvent]) -> Outcome {
        let mut last = Outcome::Continue;
        for &k in keys {
            last = p.handle(k);
        }
        last
    }

    #[test]
    fn vim_keys_and_arrows_move_the_same_way() {
        let mut vim = picker(10);
        let mut arrows = picker(10);
        press(&mut vim, &[ch('j'), ch('j'), ch('j'), ch('k')]);
        press(
            &mut arrows,
            &[
                key(KeyCode::Down),
                key(KeyCode::Down),
                key(KeyCode::Down),
                key(KeyCode::Up),
            ],
        );
        assert_eq!(vim.cursor, 2);
        assert_eq!(arrows.cursor, 2);
    }

    #[test]
    fn a_long_list_scrolls_so_every_row_can_be_reached() {
        let mut p = picker(71);
        press(&mut p, &[ch('G')]);
        assert_eq!(p.cursor, 70);
        assert_eq!(p.offset, 66, "the last page is on screen");
        press(&mut p, &[ch(' ')]);
        assert!(!p.items[70].checked);
        press(&mut p, &[ch('g')]);
        assert_eq!((p.cursor, p.offset), (0, 0));
        // Moving never runs off either end.
        press(&mut p, &[ch('k'), key(KeyCode::PageUp)]);
        assert_eq!(p.cursor, 0);
    }

    #[test]
    fn space_toggles_and_enter_confirms_the_checked_rows() {
        let mut p = picker(4);
        press(&mut p, &[ch(' '), ch('j'), ch('x')]);
        assert_eq!(press(&mut p, &[key(KeyCode::Enter)]), Outcome::Confirm);
        assert_eq!(p.checked(), vec![2, 3]);
    }

    #[test]
    fn typing_into_the_filter_never_triggers_list_commands() {
        let mut p = picker(12);
        // "j", "q", "a", "n", "g" and space are filter text here, not commands.
        let out = press(&mut p, &[ch('/'), ch('1'), ch('j'), ch('q')]);
        assert_eq!(out, Outcome::Continue);
        assert_eq!(p.filter, "1jq");
        assert!(p.visible().is_empty());
        press(&mut p, &[key(KeyCode::Backspace), key(KeyCode::Backspace)]);
        assert_eq!(p.visible(), vec![1, 10, 11]);
        // Enter leaves the filter in place; n then unchecks only what it shows.
        press(&mut p, &[key(KeyCode::Enter), ch('n')]);
        assert_eq!(p.checked().len(), 9);
        assert!(!p.items[10].checked && p.items[0].checked);
        // Esc clears the filter; it does not cancel.
        assert_eq!(press(&mut p, &[key(KeyCode::Esc)]), Outcome::Continue);
        assert_eq!(p.visible().len(), 12);
        press(&mut p, &[ch('a')]);
        assert_eq!(p.checked().len(), 12);
    }

    #[test]
    fn the_filter_matches_warnings_too() {
        let mut p = Picker::new(vec![
            PickItem::new(".zshrc", false).with_warning(Some("runs at shell startup".into())),
            PickItem::new(".config/herdr/config.toml", true),
        ]);
        press(
            &mut p,
            &[ch('/'), ch('s'), ch('h'), ch('e'), ch('l'), ch('l')],
        );
        assert_eq!(p.visible(), vec![0]);
    }

    #[test]
    fn q_and_ctrl_c_cancel() {
        assert_eq!(press(&mut picker(3), &[ch('q')]), Outcome::Cancel);
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(press(&mut picker(3), &[ch('/'), ctrl_c]), Outcome::Cancel);
    }
}
