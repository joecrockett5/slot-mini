//! MENU on the PC: the rows the PC app answers for itself. The shared page
//! draws them and says which was chosen; crates/slot-mini-pc does the work
//! (dialogs, the window, RetroArch) and hands back what the rows show.

use slot_mini_gfx::Button;

use super::{About, Outcome, Row};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HostRow {
    /// Copy games, labels and saves from an Onion SD card.
    Import,
    Update,
    Quit,
    Fullscreen,
    /// Controllers by position (Nintendo) or by their Xbox letters.
    Controller,
    /// Opens the folders page.
    Folders,
    RetroArch,
    Library,
    /// Fetch the cores RetroArch is missing.
    Cores,
    /// Opens the sync page, which the host lays out (`Host::page`).
    Sync,
    /// Remove the cart on the shelf everywhere (the host asks first).
    Remove,
    /// A row of the host's page was chosen (A).
    Page(usize),
    /// A row of the host's page was turned: Right is `true`.
    PageTurn(usize, bool),
    /// B on the host's page.
    PageBack,
}

/// A page the host lays out itself: the shared app draws it and says
/// which row was chosen. With a `title` it is a sheet of its own, without
/// the sticker: a question, a code, a list with its story.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    /// Which page this is: a different id starts on the page's `start`
    /// row, the same one keeps the row the player is on.
    pub id: u32,
    pub start: usize,
    pub title: Option<String>,
    /// Large type under the title: the pairing code.
    pub big: Option<String>,
    /// Small type under that, a line each.
    pub lines: Vec<String>,
    pub rows: Vec<PageRow>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageRow {
    pub label: String,
    /// The line under the rows while this one is selected.
    pub note: Option<String>,
}

impl PageRow {
    pub fn new(label: impl Into<String>) -> PageRow {
        PageRow {
            label: label.into(),
            note: None,
        }
    }

    pub fn note(mut self, note: impl Into<String>) -> PageRow {
        self.note = Some(note.into());
        self
    }
}

/// What the PC app's rows show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Host {
    /// The PC app's own version, printed on the sticker.
    pub version: String,
    pub fullscreen: bool,
    pub xbox: bool,
    /// Where RetroArch is, if it was found.
    pub retroarch: Option<String>,
    pub library: String,
    /// The update row's text and the line under the rows while it is
    /// selected.
    pub update: String,
    pub update_note: Option<String>,
    /// The cores row's text and note; no row while every core is there.
    pub cores: Option<String>,
    pub cores_note: Option<String>,
    /// The import row's text, and how the import is going or what it did.
    pub import: String,
    pub import_note: Option<String>,
    /// The page the host is showing over MENU's own, if any.
    pub page: Option<Page>,
    /// The game MENU was opened over, when it can be removed everywhere.
    pub remove: Option<String>,
}

/// MENU's first page on the PC.
pub(super) fn main(host: &Host) -> Vec<Row> {
    let mut rows = vec![Row::Settings, Row::Host(HostRow::Import)];
    if host.remove.is_some() {
        rows.push(Row::Host(HostRow::Remove));
    }
    rows.extend([Row::Host(HostRow::Update), Row::Host(HostRow::Quit)]);
    rows
}

pub(super) const SETTINGS: [Row; 7] = [
    Row::Sounds,
    Row::Recents,
    Row::OnlineLabels,
    Row::Host(HostRow::Fullscreen),
    Row::Host(HostRow::Controller),
    Row::Host(HostRow::Folders),
    Row::Host(HostRow::Sync),
];

/// A sheet's measures: where its title starts, and the height of the
/// title, the large type and each small line.
pub(super) const SHEET_TOP: f32 = 22.0;
pub(super) const SHEET_TITLE_H: f32 = 40.0;
pub(super) const SHEET_BIG_H: f32 = 84.0;
pub(super) const SHEET_LINE_PITCH: f32 = 24.0;

/// A path short enough for the note line: its start and its end.
pub fn short_path(path: &str) -> String {
    const MAX: usize = 64;
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= MAX {
        return path.to_string();
    }
    let head: String = chars[..20].iter().collect();
    let tail: String = chars[chars.len() - (MAX - 23)..].iter().collect();
    format!("{head}...{tail}")
}

impl About {
    /// MENU for the PC app.
    pub fn for_host(&mut self, host: Host) {
        if self.host.is_none() {
            self.rows = main(&host);
        }
        self.set_host(host);
    }

    /// The host's rows changed.
    pub fn set_host(&mut self, host: Host) {
        // The cores row comes and goes with the cores.
        if self.rows.contains(&Row::Host(HostRow::RetroArch)) {
            self.rows = folders(&host);
            self.row = self.row.min(self.rows.len() - 1);
        }
        // So does the row for the game MENU was opened over.
        if self.rows.contains(&Row::Host(HostRow::Quit)) {
            self.rows = main(&host);
            self.row = self.row.min(self.rows.len() - 1);
        }
        let rows = |page: &Page| -> Vec<Row> {
            (0..page.rows.len())
                .map(|i| Row::Host(HostRow::Page(i)))
                .collect()
        };
        match (&host.page, self.paged) {
            (Some(page), None) => {
                self.open(&rows(page));
                self.row = page.start.min(page.rows.len().saturating_sub(1));
                self.paged = Some(page.id);
            }
            (Some(page), Some(id)) => {
                self.rows = rows(page);
                if id != page.id {
                    self.row = page.start;
                    self.note = None;
                }
                self.row = self.row.min(page.rows.len().saturating_sub(1));
                self.paged = Some(page.id);
            }
            (None, Some(_)) => {
                self.back();
                self.paged = None;
            }
            (None, None) => {}
        }
        self.host = Some(host);
    }

    /// The host's page, while it shows.
    pub fn page(&self) -> Option<&Page> {
        self.paged?;
        self.host.as_ref()?.page.as_ref()
    }

    /// The host's page when it is a sheet of its own.
    pub(super) fn sheet(&self) -> Option<&Page> {
        self.page().filter(|p| p.title.is_some())
    }

    /// Where a sheet's rows start.
    pub(super) fn sheet_rows_top(page: &Page) -> f32 {
        let mut y = SHEET_TOP + SHEET_TITLE_H;
        if page.big.is_some() {
            y += SHEET_BIG_H;
        }
        y + page.lines.len() as f32 * SHEET_LINE_PITCH + 10.0
    }

    pub(super) fn host_press(&mut self, row: HostRow, b: Button) -> Outcome {
        let a = b == Button::A;
        match row {
            HostRow::Folders if a => {
                let rows = self.host.as_ref().map(folders).unwrap_or_default();
                self.open(&rows);
            }
            HostRow::Folders => {}
            HostRow::Page(i) if !a => {
                return Outcome::Host(HostRow::PageTurn(i, b == Button::Right));
            }
            // Switches turn with Left and Right as well.
            HostRow::Fullscreen | HostRow::Controller => {
                self.note = None;
                return Outcome::Host(row);
            }
            _ if a => {
                self.note = None;
                return Outcome::Host(row);
            }
            _ => {}
        }
        Outcome::Stay
    }

    pub(super) fn host_label(&self, row: HostRow) -> String {
        let Some(host) = &self.host else {
            return String::new();
        };
        match row {
            HostRow::Import => host.import.clone(),
            HostRow::Update => host.update.clone(),
            HostRow::Quit => "Quit".into(),
            HostRow::Fullscreen => {
                format!("Fullscreen: {}", if host.fullscreen { "On" } else { "Off" })
            }
            HostRow::Controller => format!(
                "Controller: {}",
                if host.xbox { "Xbox" } else { "Nintendo" }
            ),
            HostRow::Folders => "Folders".into(),
            HostRow::RetroArch => match host.retroarch {
                Some(_) => "RetroArch".into(),
                None => "RetroArch: not found".into(),
            },
            HostRow::Library => "Library folder".into(),
            HostRow::Cores => host.cores.clone().unwrap_or_default(),
            HostRow::Sync => "Sync".into(),
            HostRow::Remove => "Remove this game".into(),
            HostRow::Page(i) => host
                .page
                .as_ref()
                .and_then(|p| p.rows.get(i))
                .map(|r| r.label.clone())
                .unwrap_or_default(),
            HostRow::PageTurn(..) | HostRow::PageBack => String::new(),
        }
    }

    /// The line under the rows for the selected row.
    pub(super) fn host_note(&self) -> Option<String> {
        let host = self.host.as_ref()?;
        let Row::Host(row) = *self.rows.get(self.row)? else {
            return None;
        };
        match row {
            HostRow::Import => Some(host.import_note.clone().unwrap_or_else(|| {
                "Copies games, labels, art and saves from an Onion card.".into()
            })),
            HostRow::Update => host.update_note.clone(),
            HostRow::Controller => Some(
                match host.xbox {
                    true => "By letter: A is the bottom button, as on an Xbox pad.",
                    false => "By position: A is the right button, as on the Miyoo.",
                }
                .into(),
            ),
            HostRow::RetroArch => Some(match &host.retroarch {
                Some(path) => short_path(path),
                None => "Choose retroarch.exe where you installed it.".into(),
            }),
            HostRow::Library => Some(short_path(&host.library)),
            HostRow::Cores => host.cores_note.clone(),
            HostRow::Sync => Some("Games, labels and saves, shared with your Miyoo.".into()),
            HostRow::Remove => Some(format!(
                "{}: from this PC and every paired Miyoo. Asks first.",
                host.remove.as_deref().unwrap_or_default()
            )),
            HostRow::Page(i) => host.page.as_ref()?.rows.get(i)?.note.clone(),
            _ => None,
        }
    }
}

/// The folders page: RetroArch, the library, and the cores row while a
/// core is missing.
fn folders(host: &Host) -> Vec<Row> {
    let mut rows = vec![Row::Host(HostRow::RetroArch), Row::Host(HostRow::Library)];
    if host.cores.is_some() {
        rows.push(Row::Host(HostRow::Cores));
    }
    rows
}
