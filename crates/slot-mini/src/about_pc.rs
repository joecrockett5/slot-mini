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
    /// Opens the sync page.
    Sync,
    /// The sync page's only row until sync is built.
    SyncSoon,
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
}

/// MENU's first page on the PC.
pub(super) const MAIN: [Row; 4] = [
    Row::Settings,
    Row::Host(HostRow::Import),
    Row::Host(HostRow::Update),
    Row::Host(HostRow::Quit),
];

pub(super) const SETTINGS: [Row; 7] = [
    Row::Sounds,
    Row::Recents,
    Row::OnlineLabels,
    Row::Host(HostRow::Fullscreen),
    Row::Host(HostRow::Controller),
    Row::Host(HostRow::Folders),
    Row::Host(HostRow::Sync),
];

const SYNC: [Row; 1] = [Row::Host(HostRow::SyncSoon)];

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
            self.rows = MAIN.to_vec();
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
        self.host = Some(host);
    }

    pub(super) fn host_press(&mut self, row: HostRow, b: Button) -> Outcome {
        let a = b == Button::A;
        match row {
            HostRow::Folders if a => {
                let rows = self.host.as_ref().map(folders).unwrap_or_default();
                self.open(&rows);
            }
            HostRow::Sync if a => self.open(&SYNC),
            HostRow::Folders | HostRow::Sync | HostRow::SyncSoon => {}
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
            HostRow::SyncSoon => "Sync: coming soon".into(),
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
            HostRow::SyncSoon | HostRow::Sync => {
                Some("Saves will sync with your Miyoo in a later release.".into())
            }
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
