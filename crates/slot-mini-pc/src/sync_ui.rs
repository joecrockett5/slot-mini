//! Sync on the PC's screens: the Sync page in Settings and the sheets under
//! it (the PC's name, pairing, devices, conflicts, Remove everywhere), the
//! status icon beside the clock, and what a sync says when it ends.
//!
//! The shared app only draws a page and says which row was chosen
//! (`slot_mini::about::Page`); what the pages are and what their rows do is
//! here.

use slot_mini::about::{Page, PageRow};
use slot_mini_gfx::Sprite;
use slot_mini_sync::{Conflict, DeviceInfo, Event, GameId, HubEvent, Key, Kind, Report, Side};

use super::PcApp;
use crate::sync;

/// How long "Sync now" waits for a Miyoo to start before saying that none
/// answered.
const ANSWER_MS: u64 = 8_000;
/// A sync that has said nothing for this long has stopped.
const STALL_MS: u64 = 45_000;
/// The most rows a page without a title holds under the sticker.
const LIST_MAX: usize = 6;

#[derive(Clone, Debug, PartialEq)]
pub enum Screen {
    Main,
    /// The PC's name, being typed.
    Name(String),
    Pairing,
    Devices,
    Device(String),
    Unpair(String),
    Conflicts,
    Conflict(String),
    Dismiss(String),
    /// Remove everywhere, asked of a cart: the game and what it is called.
    Remove(GameId, String),
    Firewall,
}

/// Where pairing has got to.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Pairing {
    /// Waiting for a Miyoo to ask.
    #[default]
    Waiting,
    /// A Miyoo asked: both screens show the code.
    Code {
        device: String,
        code: String,
    },
    /// Confirmed here; the Miyoo has still to.
    Confirmed {
        device: String,
        code: String,
    },
    Paired(String),
    Failed(String),
}

/// A sync under way.
#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub device: String,
    /// The item on its way, counted from 1, of how many.
    pub item: Option<(usize, usize)>,
    pub text: String,
    /// Bytes moved in the items before this one.
    pub bytes: u64,
    /// When it last said anything.
    pub at: u64,
}

/// What a row does.
#[derive(Clone, Debug, PartialEq)]
enum Do {
    Open(Screen),
    Back,
    Poke,
    SaveName,
    Confirm,
    Deny,
    PairAgain,
    Unpair(String),
    Swap(String),
    Dismiss(String),
    Remove(GameId),
}

/// What the status icon shows.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Paired, nothing happening.
    Idle,
    Syncing(Option<(usize, usize)>),
    Conflict,
    Failed,
}

#[derive(Default)]
pub struct SyncUi {
    /// The pages B leads back through; empty when none is up.
    stack: Vec<Screen>,
    /// MENU was opened straight onto a page: leaving the page closes it.
    solo: bool,
    pub pairing: Pairing,
    pub progress: Option<Progress>,
    /// How the last sync ended, when it ended badly.
    pub failed: Option<String>,
    /// What the last sync or "Sync now" came to, for the Sync page.
    pub last: Option<String>,
    /// When "Sync now" was pressed and nothing has answered yet.
    asked: Option<u64>,
    /// What the icon last showed, and in which ink.
    icon: Option<(Option<Status>, [u8; 3])>,
    /// The games with a conflict open, as the shelf last heard.
    marked: Vec<GameId>,
    /// The game MENU's rows were last laid out over.
    over: Option<String>,
}

impl SyncUi {
    pub fn screen(&self) -> Option<&Screen> {
        self.stack.last()
    }
}

/// "3 minutes ago", for a Unix time.
pub fn ago(then: u64, now: u64) -> String {
    if then == 0 {
        return "never".into();
    }
    let secs = now.saturating_sub(then);
    let plural = |n: u64, unit: &str| match n {
        1 => format!("1 {unit} ago"),
        n => format!("{n} {unit}s ago"),
    };
    match secs {
        0..=89 => "just now".into(),
        90..=3599 => plural(secs / 60, "minute"),
        3600..=172_799 => plural(secs / 3600, "hour"),
        _ => plural(secs / 86_400, "day"),
    }
}

/// "16.2 MB".
pub fn size(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{} KB", bytes.div_ceil(1024)),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

/// What a key is to a player: "Pokemon Emerald", "Pokemon Emerald (save)".
pub fn item_name(key: &str) -> String {
    let Some(key) = Key::parse(key) else {
        return key.to_string();
    };
    let stem = key.stem().rsplit('/').next().unwrap_or("").to_string();
    match key.kind {
        Kind::Rom => stem,
        Kind::Save => format!("{stem} (save)"),
        Kind::Label => format!("{stem} (label)"),
        Kind::Art => format!("{stem} (art)"),
    }
}

fn count(n: usize, one: &str) -> String {
    match n {
        1 => format!("1 {one}"),
        n => format!("{n} {one}s"),
    }
}

/// One side's haul: "12 games and 3 saves".
fn haul(moved: &slot_mini_sync::Moved) -> Option<String> {
    let kind = |k: &String| Key::parse(k).map(|k| k.kind);
    let all = || moved.added.iter().chain(&moved.updated);
    let games = moved
        .added
        .iter()
        .filter(|k| kind(k) == Some(Kind::Rom))
        .count();
    let saves = all()
        .filter_map(|k| Key::parse(k))
        .filter(|k| k.kind == Kind::Save && k.save_ext() == "srm")
        .count();
    let mut parts = Vec::new();
    if games > 0 {
        parts.push(count(games, "game"));
    }
    if saves > 0 {
        parts.push(count(saves, "save"));
    }
    if parts.is_empty() && all().next().is_some() {
        parts.push("labels".to_string());
    }
    (!parts.is_empty()).then(|| parts.join(" and "))
}

/// A sync in a line, as the shelf says it: what came, what went, and what
/// wants a look.
pub fn summary(device: &str, report: &Report) -> String {
    if let Some(why) = &report.interrupted {
        return format!("Sync with {device} stopped: {why}");
    }
    if report.is_noop() {
        return format!("In step with {device}");
    }
    let mut parts = Vec::new();
    if let Some(got) = haul(&report.to_hub) {
        parts.push(format!("{got} in"));
    }
    if let Some(sent) = haul(&report.to_client) {
        parts.push(format!("{sent} out"));
    }
    if !report.removed.is_empty() {
        parts.push(format!("{} removed", report.removed.len()));
    }
    if !report.conflicts.is_empty() {
        parts.push(count(report.conflicts.len(), "conflict"));
    }
    if !report.failed.is_empty() {
        parts.push(format!("{} failed", report.failed.len()));
    }
    if !report.didnt_fit.is_empty() {
        parts.push(format!("{} did not fit the card", report.didnt_fit.len()));
    }
    if !report.busy.is_empty() {
        parts.push("the running game was left alone".to_string());
    }
    if parts.is_empty() {
        return format!("Synced with {device}");
    }
    format!("Synced with {device}: {}", parts.join(", "))
}

/// Whether a sync ended in a way the icon should keep saying.
fn went_wrong(report: &Report) -> bool {
    report.interrupted.is_some() || !report.failed.is_empty()
}

/// The game a conflict is about, and its name.
fn conflict_game(c: &Conflict) -> Option<GameId> {
    Key::parse(&c.key).map(|k| k.game())
}

fn conflict_name(c: &Conflict) -> String {
    Key::parse(&c.key).map_or_else(|| c.key.clone(), |k| k.stem().to_string())
}

fn played(t: Option<u64>, now: u64) -> String {
    match t {
        Some(t) => format!("last played {}", ago(t, now)),
        None => "last played: not known".to_string(),
    }
}

impl PcApp {
    fn hub(&self) -> Option<&std::sync::Arc<slot_mini_sync::Hub>> {
        self.sync.as_ref().map(|s| s.hub())
    }

    fn devices(&self) -> Vec<DeviceInfo> {
        let mut devices = self.hub().map(|h| h.devices()).unwrap_or_default();
        devices.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
        devices
    }

    fn conflicts(&self) -> Vec<Conflict> {
        self.hub().map(|h| h.conflicts()).unwrap_or_default()
    }

    /// The first paired device's name, or what to call one.
    fn a_device(&self) -> String {
        self.devices()
            .first()
            .map_or("your Miyoo".to_string(), |d| d.name.clone())
    }

    // ---- pages ---------------------------------------------------------

    /// MENU's Sync row.
    pub(super) fn open_sync(&mut self) {
        self.ui.stack = vec![Screen::Main];
        self.ui.solo = false;
    }

    /// Open MENU from the shelf straight onto `screen`.
    pub(super) fn open_solo(&mut self, screen: Screen) -> bool {
        if !self.app.at_rest() {
            return false;
        }
        if screen == Screen::Pairing {
            self.start_pairing();
        }
        self.ui.stack = vec![screen];
        self.ui.solo = true;
        self.refresh_host();
        self.app.open_menu()
    }

    /// The first-run screen's offer was taken: pairing, or the Sync page
    /// once a Miyoo is paired.
    pub(super) fn pair_from_shelf(&mut self) -> bool {
        let paired = self.hub().is_some_and(|h| !h.devices().is_empty());
        self.open_solo(match paired {
            true => Screen::Main,
            false => Screen::Pairing,
        })
    }

    /// What the first-run screen says about pairing.
    pub(super) fn offer(&self) -> Option<slot_mini::about::Offer> {
        let hub = self.hub()?;
        Some(slot_mini::about::Offer {
            paired: hub.devices().first().map(|d| d.name.clone()),
            note: match &self.ui.progress {
                Some(p) => Some(p.text.clone()),
                None => self.ui.failed.clone(),
            },
        })
    }

    /// Ask about removing the cart on the shelf everywhere: from MENU's
    /// row, a right click on the cart, or Delete.
    pub fn remove_current(&mut self) -> bool {
        if self.sync.is_none() || self.in_game() {
            return false;
        }
        let Some((_, cart)) = self.app.current() else {
            return false;
        };
        let Some(game) = sync::game_of(cart) else {
            return false;
        };
        let screen = Screen::Remove(game, slot_mini_ui::label_text(cart));
        if self.app.about.is_some() {
            // From MENU: B leads back to it.
            self.ui.stack = vec![screen];
            self.ui.solo = false;
            self.refresh_host();
            return true;
        }
        self.open_solo(screen)
    }

    /// The game MENU's "Remove this game" row is about.
    pub(super) fn removable(&self) -> Option<String> {
        self.sync.as_ref()?;
        let (_, cart) = self.app.current()?;
        sync::game_of(cart)?;
        Some(slot_mini_ui::label_text(cart))
    }

    fn push(&mut self, screen: Screen) {
        if screen == Screen::Pairing {
            self.start_pairing();
        }
        self.ui.stack.push(screen);
    }

    fn start_pairing(&mut self) {
        self.ui.pairing = Pairing::Waiting;
        if let Some(hub) = self.hub() {
            hub.start_pairing();
        }
    }

    /// B, or a row that leads back.
    pub(super) fn page_back(&mut self) {
        if self.ui.stack.pop() == Some(Screen::Pairing) {
            if let Some(hub) = self.hub() {
                hub.stop_pairing();
            }
        }
        if self.ui.stack.is_empty() && self.ui.solo {
            self.ui.solo = false;
            self.app.close_menu();
        }
    }

    /// MENU closed with a page up.
    pub(super) fn leave_pages(&mut self) {
        if self.ui.stack.contains(&Screen::Pairing) {
            if let Some(hub) = self.hub() {
                hub.stop_pairing();
            }
        }
        self.ui.stack.clear();
        self.ui.solo = false;
    }

    /// The top page's rows and what each does.
    fn rows(&self) -> Vec<(Do, PageRow)> {
        let now = slot_mini_sync::now_secs();
        let row = |act: Do, label: &str| (act, PageRow::new(label));
        let Some(screen) = self.ui.screen() else {
            return Vec::new();
        };
        let Some(service) = &self.sync else {
            return vec![(
                Do::Back,
                PageRow::new("Sync is not available")
                    .note("Its files in the library could not be opened."),
            )];
        };
        let hub = service.hub();
        match screen {
            Screen::Main => {
                let devices = self.devices();
                let conflicts = self.conflicts();
                let mut rows = vec![(
                    Do::Open(Screen::Name(hub.name().to_string())),
                    PageRow::new(format!("This PC: {}", hub.name()))
                        .note("The name your Miyoo shows for this PC."),
                )];
                let sync_note = if let Some(why) = &service.offline {
                    format!("Sync is off: {why}")
                } else if let Some(p) = &self.ui.progress {
                    p.text.clone()
                } else if self.ui.asked.is_some() {
                    format!("Asking {}...", self.a_device())
                } else if let Some(last) = &self.ui.last {
                    last.clone()
                } else if devices.is_empty() {
                    "Pair a Miyoo first.".to_string()
                } else {
                    "Asks your Miyoo to sync. It must be awake, on Wi-Fi, on its shelf.".to_string()
                };
                rows.push((Do::Poke, PageRow::new("Sync now").note(sync_note)));
                rows.push((
                    Do::Open(Screen::Pairing),
                    PageRow::new("Pair a Miyoo")
                        .note("Shows the code to check against the Miyoo's screen."),
                ));
                if !devices.is_empty() {
                    let names: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
                    rows.push((
                        Do::Open(Screen::Devices),
                        PageRow::new(format!("Devices: {}", devices.len())).note(names.join(", ")),
                    ));
                }
                if !conflicts.is_empty() {
                    rows.push((
                        Do::Open(Screen::Conflicts),
                        PageRow::new(format!("Conflicts: {}", conflicts.len()))
                            .note("Saves that changed on both sides. Both copies are kept."),
                    ));
                }
                rows.push((
                    Do::Open(Screen::Firewall),
                    PageRow::new("Miyoo can't find this PC?")
                        .note("What Windows Firewall asks, and how to let sync through."),
                ));
                rows
            }
            Screen::Name(text) => vec![
                (
                    Do::SaveName,
                    PageRow::new(format!("{text}_")).note("Type on the keyboard. Enter saves."),
                ),
                row(Do::SaveName, "Save"),
                row(Do::Back, "Cancel"),
            ],
            Screen::Pairing => match &self.ui.pairing {
                Pairing::Waiting => vec![row(Do::Back, "Cancel")],
                Pairing::Code { .. } => vec![
                    (
                        Do::Confirm,
                        PageRow::new("Confirm").note("Only if both screens show the same code."),
                    ),
                    (
                        Do::Deny,
                        PageRow::new("Deny").note("The Miyoo is not paired."),
                    ),
                ],
                Pairing::Confirmed { .. } => vec![row(Do::Deny, "Cancel")],
                Pairing::Paired(_) => vec![row(Do::Back, "Done")],
                Pairing::Failed(_) => {
                    vec![row(Do::PairAgain, "Try again"), row(Do::Back, "Back")]
                }
            },
            Screen::Devices => {
                let mut rows: Vec<(Do, PageRow)> = self
                    .devices()
                    .into_iter()
                    .take(LIST_MAX)
                    .map(|d| {
                        let note = format!("Last sync: {}", ago(d.last_sync, now));
                        (
                            Do::Open(Screen::Device(d.id.clone())),
                            PageRow::new(d.name).note(note),
                        )
                    })
                    .collect();
                if rows.is_empty() {
                    rows.push((
                        Do::Back,
                        PageRow::new("No devices").note("Pair a Miyoo to see it here."),
                    ));
                }
                rows
            }
            Screen::Device(id) => vec![
                (
                    Do::Open(Screen::Unpair(id.clone())),
                    PageRow::new("Unpair").note("Stops this Miyoo syncing with this PC."),
                ),
                row(Do::Back, "Back"),
            ],
            Screen::Unpair(id) => vec![
                row(Do::Back, "Cancel"),
                row(Do::Unpair(id.clone()), "Unpair"),
            ],
            Screen::Conflicts => {
                let mut rows: Vec<(Do, PageRow)> = self
                    .conflicts()
                    .into_iter()
                    .take(LIST_MAX)
                    .map(|c| {
                        let note = format!("Found syncing with {}.", c.device);
                        (
                            Do::Open(Screen::Conflict(c.id.clone())),
                            PageRow::new(conflict_name(&c)).note(note),
                        )
                    })
                    .collect();
                if rows.is_empty() {
                    rows.push((
                        Do::Back,
                        PageRow::new("No conflicts").note("Every save is in step."),
                    ));
                }
                rows
            }
            Screen::Conflict(id) => vec![
                (
                    Do::Swap(id.clone()),
                    PageRow::new("Use the other copy")
                        .note("The two change places. Nothing is deleted."),
                ),
                (
                    Do::Open(Screen::Dismiss(id.clone())),
                    PageRow::new("Keep this one").note("Settles it: the other copy is put away."),
                ),
                row(Do::Back, "Back"),
            ],
            Screen::Dismiss(id) => vec![
                row(Do::Back, "Cancel"),
                row(Do::Dismiss(id.clone()), "Put the other copy away"),
            ],
            Screen::Remove(game, _) => vec![
                row(Do::Back, "Cancel"),
                row(Do::Remove(game.clone()), "Remove everywhere"),
            ],
            Screen::Firewall => vec![row(Do::Back, "Back")],
        }
    }

    /// The page MENU shows for the top screen.
    pub(super) fn sync_page(&self) -> Option<Page> {
        let screen = self.ui.screen()?;
        let now = slot_mini_sync::now_secs();
        let name = self.hub().map_or("this PC", |h| h.name()).to_string();
        let lines = |text: &[&str]| text.iter().map(|l| l.to_string()).collect::<Vec<_>>();
        let mut page = Page {
            id: self.ui.stack.len() as u32 * 16
                + match screen {
                    Screen::Main => 0,
                    Screen::Name(_) => 1,
                    Screen::Pairing => 2,
                    Screen::Devices => 3,
                    Screen::Device(_) => 4,
                    Screen::Unpair(_) => 5,
                    Screen::Conflicts => 6,
                    Screen::Conflict(_) => 7,
                    Screen::Dismiss(_) => 8,
                    Screen::Remove(..) => 9,
                    Screen::Firewall => 10,
                },
            rows: self.rows().into_iter().map(|(_, row)| row).collect(),
            ..Page::default()
        };
        match screen {
            Screen::Main | Screen::Devices | Screen::Conflicts => {}
            Screen::Name(_) => {
                page.title = Some("This PC's name".into());
                page.lines = lines(&[
                    "Your Miyoo shows this name when it looks for a PC and when it syncs.",
                    "Paired Miyoos stay paired: they learn the new name at their next sync.",
                ]);
            }
            Screen::Pairing => match &self.ui.pairing {
                Pairing::Waiting => {
                    page.title = Some("Pair a Miyoo".into());
                    page.lines = vec![
                        "On the Miyoo, open MENU, choose Sync, and pair with:".into(),
                        name,
                        String::new(),
                        "Both need to be on the same Wi-Fi network.".into(),
                        "Waiting for the Miyoo...".into(),
                    ];
                }
                Pairing::Code { device, code } => {
                    page.title = Some(format!("Pair with {device}?"));
                    page.big = Some(code.clone());
                    page.lines = lines(&[
                        "Check that the Miyoo shows the same code.",
                        "If it does not, something else is asking: deny it.",
                    ]);
                }
                Pairing::Confirmed { device, code } => {
                    page.title = Some(format!("Pair with {device}?"));
                    page.big = Some(code.clone());
                    page.lines = lines(&["Confirmed here. Now confirm on the Miyoo."]);
                }
                Pairing::Paired(device) => {
                    page.title = Some(format!("Paired with {device}"));
                    page.lines = lines(&[
                        "Now choose Sync now on the Miyoo.",
                        "Its games, labels and saves come over to this PC,",
                        "and from then on the two keep each other in step.",
                        "",
                    ]);
                    page.lines.push(match (&self.ui.progress, &self.ui.last) {
                        (Some(p), _) => p.text.clone(),
                        (None, Some(last)) => last.clone(),
                        (None, None) => "Waiting for the Miyoo...".into(),
                    });
                }
                Pairing::Failed(why) => {
                    page.title = Some("Not paired".into());
                    page.lines = vec![why.clone()];
                }
            },
            Screen::Device(id) | Screen::Unpair(id) => {
                let device = self.devices().into_iter().find(|d| &d.id == id);
                let Some(d) = device else {
                    page.title = Some("No longer paired".into());
                    return Some(page);
                };
                if matches!(screen, Screen::Unpair(_)) {
                    page.title = Some(format!("Unpair {}?", d.name));
                    page.lines = lines(&[
                        "It stops syncing with this PC until it is paired again.",
                        "Nothing is deleted from the Miyoo or from this PC.",
                    ]);
                    page.start = 0;
                } else {
                    page.title = Some(d.name.clone());
                    page.lines = vec![
                        format!("Paired: {}", ago(d.paired, now)),
                        format!("Last sync: {}", ago(d.last_sync, now)),
                        format!(
                            "Last address: {}",
                            if d.address.is_empty() {
                                "not known"
                            } else {
                                &d.address
                            }
                        ),
                    ];
                }
            }
            Screen::Conflict(id) | Screen::Dismiss(id) => {
                let Some(c) = self.conflicts().into_iter().find(|c| &c.id == id) else {
                    page.title = Some("No longer a conflict".into());
                    return Some(page);
                };
                // The copy set aside came from one side; the save in use
                // from the other.
                let (aside, used) = match c.from {
                    Side::Hub => ("this PC".to_string(), c.device.clone()),
                    Side::Client => (c.device.clone(), "this PC".to_string()),
                };
                let mut text = vec![
                    "This save changed on both sides, so both copies are kept.".to_string(),
                    String::new(),
                    format!("In use: the copy from {used},"),
                    played(c.save_played, now),
                    format!("Set aside: the copy from {aside},"),
                    played(c.played, now),
                ];
                if matches!(screen, Screen::Dismiss(_)) {
                    page.title = Some(format!("Keep the copy from {used}?"));
                    text.push(String::new());
                    text.push(format!(
                        "The copy from {aside} moves to Saves/slot-mini-removed/{}",
                        Key::parse(&c.key).map_or("", |k| k.sys.dir_name())
                    ));
                    text.push("in the library, and the game stops showing a conflict.".into());
                } else {
                    page.title = Some(conflict_name(&c));
                }
                page.lines = text;
            }
            Screen::Remove(game, title) => {
                let sys = game.sys.dir_name();
                page.title = Some(format!("Remove {title} everywhere?"));
                page.lines = vec![
                    "The game, its label and its art are deleted from this PC now,".into(),
                    "and from each paired Miyoo at its next sync.".into(),
                    String::new(),
                    "Saves are not deleted. On each side they move to".into(),
                    format!("Saves/slot-mini-removed/{sys}"),
                    "(on this PC, inside the library folder).".into(),
                    String::new(),
                    "Copying the game back later brings it back as a new game.".into(),
                ];
            }
            Screen::Firewall => {
                page.title = Some("If the Miyoo can't find this PC".into());
                page.lines = lines(&[
                    "The first time slot-mini runs, Windows Firewall asks whether to",
                    "let it through. Choose Allow, with Private networks ticked:",
                    "sync only ever talks to your own Wi-Fi network.",
                    "",
                    "If you chose Cancel, open Windows Security, Firewall and network",
                    "protection, Allow an app through firewall, Change settings, and",
                    "tick slot-mini-pc under Private. Then try again from the Miyoo.",
                    "",
                    "Both must be on the same network (TCP and UDP port 24735).",
                ]);
            }
        }
        Some(page)
    }

    /// A row of the page was chosen.
    pub(super) fn page_action(&mut self, i: usize) {
        let Some((act, _)) = self.rows().into_iter().nth(i) else {
            return;
        };
        match act {
            Do::Open(screen) => self.push(screen),
            Do::Back => self.page_back(),
            Do::Poke => self.sync_now(),
            Do::SaveName => self.save_name(),
            Do::Confirm => {
                let (Some(hub), Pairing::Code { device, code }) =
                    (self.hub(), self.ui.pairing.clone())
                else {
                    return;
                };
                self.ui.pairing = match hub.confirm_pairing() {
                    true => Pairing::Confirmed { device, code },
                    false => Pairing::Failed("The Miyoo gave up waiting. Start again.".into()),
                };
            }
            Do::Deny => {
                if let Some(hub) = self.hub() {
                    hub.stop_pairing();
                }
                self.ui.pairing = Pairing::Failed("Denied: the Miyoo was not paired.".into());
            }
            Do::PairAgain => self.start_pairing(),
            Do::Unpair(id) => {
                let result = self.hub().map(|h| h.unpair(&id));
                self.page_back();
                self.page_back();
                match result {
                    Some(Err(e)) => self.say(format!("Not unpaired: {e}")),
                    _ => {
                        if self.devices().is_empty() {
                            self.page_back();
                        }
                    }
                }
            }
            Do::Swap(id) => {
                if let Some(Err(e)) = self.hub().map(|h| h.swap_conflict(&id)) {
                    self.say(format!("Not swapped: {e}"));
                } else {
                    self.say("Swapped. The Miyoo gets this copy at its next sync.".into());
                    self.rescan_due = true;
                }
                // A copy that held nothing leaves no conflict behind.
                if !self.conflicts().iter().any(|c| c.id == id) {
                    self.page_back();
                    self.leave_empty_list();
                }
            }
            Do::Dismiss(id) => {
                let result = self.hub().map(|h| h.dismiss_conflict(&id));
                self.page_back();
                self.page_back();
                self.leave_empty_list();
                if let Some(Err(e)) = result {
                    self.say(format!("Not settled: {e}"));
                }
            }
            Do::Remove(game) => {
                let busy = self.hub().is_some_and(|h| h.is_syncing());
                let result = match busy {
                    true => None,
                    false => self.hub().map(|h| h.remove_everywhere(&game)),
                };
                let said = match result {
                    None => "Syncing: try again in a moment".to_string(),
                    Some(Ok(())) => format!("Removed {}", game.stem),
                    Some(Err(e)) => format!("Not removed: {e}"),
                };
                // Back to the shelf, which is about to lose the cart: MENU
                // was opened over it.
                self.leave_pages();
                self.app.close_menu();
                self.rescan_due = true;
                self.app.set_notice(said);
            }
        }
    }

    /// The conflicts list with nothing left on it goes back to the page
    /// above.
    fn leave_empty_list(&mut self) {
        if self.ui.screen() == Some(&Screen::Conflicts) && self.conflicts().is_empty() {
            self.page_back();
        }
    }

    /// A line under MENU's rows.
    fn say(&mut self, text: String) {
        self.app.set_about_note(Some(text));
    }

    /// Left or Right on a row: on the name, Left rubs a letter out.
    pub(super) fn page_turn(&mut self, i: usize, right: bool) {
        if i == 0 && !right {
            self.backspace();
        }
    }

    // ---- the PC's name -------------------------------------------------

    /// Whether typing goes to the page (the PC's name).
    pub fn wants_text(&self) -> bool {
        matches!(self.ui.screen(), Some(Screen::Name(_))) && self.app.about.is_some()
    }

    /// The keyboard typed `text`.
    pub fn text(&mut self, text: &str) {
        if let Some(Screen::Name(name)) = self.ui.stack.last_mut() {
            for c in text.chars().filter(|c| !c.is_control()) {
                if name.chars().count() < sync::NAME_MAX {
                    name.push(c);
                }
            }
            self.refresh_host();
        }
    }

    pub fn backspace(&mut self) {
        if let Some(Screen::Name(name)) = self.ui.stack.last_mut() {
            name.pop();
            self.refresh_host();
        }
    }

    fn save_name(&mut self) {
        let Some(Screen::Name(name)) = self.ui.screen().cloned() else {
            return;
        };
        let result = match &mut self.sync {
            Some(service) => service.rename(&name),
            None => return,
        };
        match result {
            Ok(()) => self.page_back(),
            Err(e) => self.say(e),
        }
    }

    // ---- syncing -------------------------------------------------------

    /// "Sync now": ask the paired Miyoos, and notice when none answers.
    pub(super) fn sync_now(&mut self) {
        let Some(service) = &self.sync else {
            return;
        };
        if service.hub().is_syncing() {
            return;
        }
        if service.hub().devices().is_empty() {
            self.ui.last = Some("Pair a Miyoo first.".into());
            return;
        }
        match service.poke() {
            0 => {
                self.ui.last = Some(
                    "Nothing to ask yet: sync once from the Miyoo, so this PC knows where it is."
                        .into(),
                );
            }
            _ => {
                self.ui.asked = Some(self.now);
                self.ui.last = None;
            }
        }
    }

    /// Time passing: a Miyoo that never answered, a sync that stopped.
    pub(super) fn sync_tick(&mut self) -> bool {
        let mut changed = false;
        // MENU's "Remove this game" follows the cart on the shelf.
        let over = self.removable();
        if over != self.ui.over {
            self.ui.over = over;
            changed = true;
        }
        if self.ui.asked.is_some_and(|t| self.now >= t + ANSWER_MS) {
            self.ui.asked = None;
            self.ui.last = Some(
                "No answer. Is the Miyoo awake, on Wi-Fi, and on its shelf (not in a game)?".into(),
            );
            changed = true;
        }
        let stalled = self
            .ui
            .progress
            .as_ref()
            .is_some_and(|p| self.now >= p.at + STALL_MS)
            && !self.hub().is_some_and(|h| h.is_syncing());
        if stalled {
            let device = self
                .ui
                .progress
                .take()
                .map(|p| p.device)
                .unwrap_or_default();
            let text = format!("Sync with {device} stopped part way. It carries on next time.");
            self.ui.failed = Some(text.clone());
            self.ui.last = Some(text.clone());
            self.app.set_notice(text);
            self.rescan_due = true;
            changed = true;
        }
        // MENU closed over a page (MENU, or the window's own ways out).
        if !self.ui.stack.is_empty() && self.app.about.is_none() {
            self.leave_pages();
            changed = true;
        }
        changed
    }

    /// The hub said something (on one of its own threads, a moment ago).
    pub(super) fn sync_event(&mut self, event: HubEvent) {
        match event {
            // A game, label or save arrived, or went.
            HubEvent::Changed(_) => self.rescan_due = true,
            HubEvent::Sync { device, event } => self.sync_progress(device, event),
            HubEvent::PairingCode { device_name, code } => {
                if self.ui.stack.contains(&Screen::Pairing) {
                    self.ui.pairing = Pairing::Code {
                        device: device_name,
                        code,
                    };
                }
            }
            HubEvent::Paired(device) => {
                if self.ui.stack.contains(&Screen::Pairing) {
                    self.ui.pairing = Pairing::Paired(device.name);
                    self.ui.last = None;
                    if let Some(hub) = self.hub() {
                        hub.stop_pairing();
                    }
                }
            }
            HubEvent::Unpaired(_) => {}
        }
    }

    fn sync_progress(&mut self, device: String, event: Event) {
        self.ui.asked = None;
        let (item, bytes) = match &self.ui.progress {
            Some(p) if p.device == device => (p.item, p.bytes),
            _ => (None, 0),
        };
        let mut next = Progress {
            device: device.clone(),
            item,
            text: String::new(),
            bytes,
            at: self.now,
        };
        match event {
            Event::Connecting | Event::Scanning { .. } | Event::Planning => {
                next.item = None;
                next.bytes = 0;
                next.text = format!("Syncing with {device}: comparing the two libraries...");
            }
            Event::Transfer {
                index,
                count,
                key,
                to_client,
                size: bytes_of,
                ..
            } => {
                let count = count.max(index + 1);
                next.item = Some((index + 1, count));
                next.text = format!(
                    "{} {device}: {} of {count}, {} ({}; {} so far)",
                    if to_client { "To" } else { "From" },
                    index + 1,
                    item_name(&key),
                    size(bytes_of),
                    size(bytes),
                );
                next.bytes = bytes + bytes_of;
            }
            Event::Finished(report) => {
                let text = summary(&device, &report);
                self.ui.progress = None;
                self.ui.failed = went_wrong(&report).then(|| text.clone());
                self.ui.last = Some(text.clone());
                self.rescan_due = true;
                let kind = |k: &String| Key::parse(k).map(|k| k.kind);
                let moved = &report.to_hub;
                self.relabel_due |= report.text_changed
                    || moved
                        .added
                        .iter()
                        .chain(&moved.updated)
                        .any(|k| kind(k) == Some(Kind::Label));
                if !report.is_noop() {
                    self.app.set_notice(text);
                }
                return;
            }
        }
        self.ui.failed = None;
        self.ui.progress = Some(next);
    }

    // ---- the status icon and the shelf's marks -------------------------

    pub fn sync_status(&self) -> Option<Status> {
        let hub = self.hub()?;
        if let Some(p) = &self.ui.progress {
            return Some(Status::Syncing(p.item));
        }
        if self.ui.failed.is_some() {
            return Some(Status::Failed);
        }
        if !hub.conflicts().is_empty() {
            return Some(Status::Conflict);
        }
        (!hub.devices().is_empty()).then_some(Status::Idle)
    }

    /// Show the sync's state beside the clock, when it has changed.
    pub(super) fn show_status(&mut self) {
        let status = self.sync_status();
        let ink = self.app.theme.footer_ink;
        let shown = Some((status.clone(), ink));
        if self.ui.icon == shown {
            return;
        }
        self.ui.icon = shown;
        self.app
            .set_status_icon(status.map(|s| status_face(&s, ink)));
        let marked: Vec<GameId> = self.conflicts().iter().filter_map(conflict_game).collect();
        if marked != self.ui.marked {
            self.ui.marked = marked;
            self.mark_conflicts();
        }
    }

    /// Tell the shelf which carts have a conflict open.
    pub(super) fn mark_conflicts(&mut self) {
        let marked = self.ui.marked.clone();
        self.app.set_marked(move |cart| {
            sync::game_of(cart).is_some_and(|g| marked.iter().any(|m| m.folded() == g.folded()))
        });
    }
}

/// The icon: two arrows, dim while nothing is happening, with the count
/// while a sync runs, `!` for a conflict and `X` for a sync that failed.
pub fn status_face(status: &Status, ink: [u8; 3]) -> Sprite {
    const PX: u32 = 18;
    let word = match status {
        Status::Idle => None,
        Status::Syncing(None) => Some("...".to_string()),
        Status::Syncing(Some((i, n))) => Some(format!("{i}/{n}")),
        Status::Conflict => Some("!".to_string()),
        Status::Failed => Some("X".to_string()),
    };
    let word = word.map(|w| slot_mini_ui::word_ink(&w, ink));
    let (ww, wh) = word.as_ref().map_or((0, 0), |w| (w.w + 5, w.h));
    let (w, h) = (PX + ww, PX.max(wh));
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let alpha = if *status == Status::Idle { 0x80 } else { 0xff };
    let top = (h - PX) / 2;
    // Up on the left, down on the right.
    let rects: [[u32; 4]; 10] = [
        [3, 0, 2, 1],
        [2, 1, 4, 1],
        [1, 2, 6, 1],
        [0, 3, 8, 1],
        [3, 4, 2, 12],
        [13, 2, 2, 12],
        [10, 14, 8, 1],
        [11, 15, 6, 1],
        [12, 16, 4, 1],
        [13, 17, 2, 1],
    ];
    for [x, y, rw, rh] in rects {
        for py in y..y + rh {
            for px in x..x + rw {
                let at = (((top + py) * w + px) * 4) as usize;
                rgba[at..at + 3].copy_from_slice(&ink);
                rgba[at + 3] = alpha;
            }
        }
    }
    if let Some(word) = word {
        let y0 = (h - word.h) / 2;
        for y in 0..word.h {
            let from = (y * word.w * 4) as usize;
            let to = (((y0 + y) * w + PX + 5) * 4) as usize;
            let n = (word.w * 4) as usize;
            rgba[to..to + n].copy_from_slice(&word.rgba[from..from + n]);
        }
    }
    Sprite::from_rgba(w, h, &rgba)
}

#[cfg(test)]
mod tests {
    use super::*;
    use slot_mini_sync::{Moved, Scope, SyncError};

    #[test]
    fn times_and_sizes_read_plainly() {
        assert_eq!(ago(0, 1000), "never");
        assert_eq!(ago(990, 1000), "just now");
        assert_eq!(ago(1000, 1000 + 60 * 5), "5 minutes ago");
        assert_eq!(ago(1000, 1000 + 3600), "1 hour ago");
        assert_eq!(ago(1000, 1000 + 86_400 * 3), "3 days ago");
        assert_eq!(size(512), "512 B");
        assert_eq!(size(4096), "4 KB");
        assert_eq!(size(16 * 1_048_576), "16.0 MB");
        assert_eq!(item_name("rom:GBA/Hacks/Emerald.gba"), "Emerald");
        assert_eq!(item_name("save:GBA/Emerald.srm"), "Emerald (save)");
    }

    #[test]
    fn a_sync_is_summed_up_in_a_line() {
        let mut report = Report::new(Scope::Full);
        assert_eq!(summary("Miyoo", &report), "In step with Miyoo");
        report.to_hub = Moved {
            added: vec![
                "rom:GBA/A.gba".into(),
                "rom:GBA/B.gba".into(),
                "label:GBA/A".into(),
                "save:GBA/A.srm".into(),
            ],
            updated: vec![],
        };
        report.to_client.updated = vec!["save:GBA/C.srm".into()];
        assert_eq!(
            summary("Miyoo", &report),
            "Synced with Miyoo: 2 games and 1 save in, 1 save out"
        );
        report.conflicts.push(("save:GBA/D.srm".into(), Side::Hub));
        report.failed.push(("rom:GBA/E.gba".into(), "no".into()));
        assert_eq!(
            summary("Miyoo", &report),
            "Synced with Miyoo: 2 games and 1 save in, 1 save out, 1 conflict, 1 failed"
        );
        assert!(went_wrong(&report));
        report.interrupted = Some(SyncError::Cancelled);
        assert!(summary("Miyoo", &report).starts_with("Sync with Miyoo stopped"));
    }
}
