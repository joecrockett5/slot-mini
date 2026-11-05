//! The launcher: slot's shelves, driven by buttons, drawn through slot's
//! layout code into a software canvas.
//!
//! The flow follows slot's `crates/slot/src/app.rs` (shelf switching,
//! favourites, tap and hold A, the insert and eject), cut down to what a
//! launcher needs. Timings and draw order are slot's.

use std::time::Duration;

use slot_mini_gfx::{Button, Canvas, Draw, Event, Renderer, Sprite, Stage, TexId, OUT_H};
use slot_mini_store::{Cart, Platform};
use slot_mini_ui::power::{Battery, Charge};
use slot_mini_ui::{
    cart_backing, cart_shadow, draw_footer, draw_ground, draw_slot_title, draw_wifi_icon, ease,
    gb_cart_shadow, heart_face, title_line_face, word_ink, GbShell, Millis, Printed, Shelf,
    SlotChrome, UndoFace, HEART_SCALE,
};

use crate::faces::{CartFaces, FaceWant};
use crate::library::{self, ShelfKind};
use crate::paths::Paths;
use crate::settings::{Favourites, Settings, State};
use crate::status::{clock_text, Status};
use crate::theme::Theme;

/// slot: `PLAY_HOLD_MS`. Holding A this long starts the game fresh.
pub const PLAY_HOLD_MS: Millis = 500;
/// slot: `INSERT_S`, `INSERT_HOLD_S`. The cart travels into the slot, then
/// the frame holds before the game starts.
pub const INSERT_S: f32 = 0.73;
const INSERT_HOLD_S: f32 = 0.28;
pub const SEATED_AT: f32 = INSERT_S - INSERT_HOLD_S;
/// slot: `EJECT_S`.
pub const EJECT_S: f32 = SEATED_AT;
/// slot: the platform name fades in, holds, and fades out.
const SLOT_NAME_IN_MS: Millis = 200;
const SLOT_NAME_HOLD_MS: Millis = 1200;
const SLOT_NAME_OUT_MS: Millis = 800;
/// slot: `BATTERY_POLL_MS`.
const STATUS_POLL_MS: Millis = 10_000;
/// Shown when no GBA, GB or GBC games were found.
const EMPTY_TEXT: &str = "No games in Roms/GBA, GBC or GB";
/// An animation that starts with the next frame shown.
const NOT_STARTED: Millis = Millis::MAX;
/// How long a frame waits for the faces it is about to show.
pub const FACE_WAIT: Duration = Duration::from_millis(500);
/// Faces kept decoded either side of the selected cart.
const NEAR: i32 = 4;

/// Why the app stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Exit {
    /// Start this game. `fresh` skips loading the auto save state.
    Launch { cart: Cart, fresh: bool },
    /// Back to Onion's own menu.
    ToOnion,
    /// The process was asked to stop.
    Quit,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Shelf,
    /// The selected cart travels into the slot.
    Inserting {
        at: Millis,
        stem: String,
        fresh: bool,
    },
    /// The cart comes back out after a game.
    Ejecting {
        at: Millis,
        stem: String,
    },
    /// SELECT: the cart studio for the selected cart.
    Studio,
    /// MENU: about slot-mini, its settings, and the way back to Onion.
    About,
}

pub struct Options {
    /// Where the insert and eject clips go. `None` picks Onion's player when
    /// `sounds = on`, silence otherwise.
    pub sound: Option<Box<dyn crate::sound::Sound>>,
    /// How long a frame may wait for on-screen cart faces, as slot waits
    /// (`FACES_WAIT_MS`). Every frame waits, so a jump far along the shelf or
    /// to another shelf pauses briefly instead of flashing label-colour
    /// placeholders. Only past the deadline does a cart show its label colour
    /// until its face lands. `None` waits as long as it takes (tests).
    pub face_wait: Option<Duration>,
    /// Picks the random wallpaper.
    pub seed: u64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            sound: None,
            face_wait: Some(FACE_WAIT),
            seed: 0,
        }
    }
}

struct Faces {
    heart: Option<(TexId, u32, u32)>,
    /// Shadows (GBA, notched, rounded) and see-through backings, shared by
    /// every shelf.
    shadows: [TexId; 3],
    backings: [TexId; 3],
    empty: (TexId, u32, u32),
    slot: [(TexId, TexId); 2],
    ground: Option<(TexId, u32)>,
    wifi: Option<(TexId, u32, u32)>,
}

#[derive(Default)]
struct Printing {
    text: String,
    printed: Printed,
}

pub struct App {
    pub paths: Paths,
    pub settings: Settings,
    pub theme: Theme,
    pub favourites: Favourites,
    carts: Vec<Cart>,
    shelves: Vec<(ShelfKind, Shelf)>,
    at: usize,
    pub gfx: Renderer,
    faces: CartFaces,
    face_wait: Option<Duration>,
    stage: Stage,
    statics: Faces,
    title: Printing,
    clock: Printing,
    percent: Printing,
    shelf_named: Option<Millis>,
    /// A message shown where the platform name goes, e.g. why a cart
    /// cannot be launched.
    notice: Option<String>,
    pub phase: Phase,
    a_held: Option<Millis>,
    status: Box<dyn Status>,
    battery: Option<u8>,
    wifi: bool,
    polled: Option<Millis>,
    now: Millis,
    draws: Vec<Draw>,
    pub studio: Option<crate::studio_ui::StudioUi>,
    pub about: Option<crate::about::About>,
    sound: Box<dyn crate::sound::Sound>,
    /// A sound sink handed in (tests) plays whatever the setting says.
    sound_given: bool,
    /// Labels arriving from slot's art service, when it is switched on.
    fetched: Option<std::sync::mpsc::Receiver<(Cart, std::path::PathBuf)>>,
    fetch_started: bool,
    restock_pending: bool,
    pub exit: Option<Exit>,
}

fn upload(gfx: &mut Renderer, f: &UndoFace) -> TexId {
    gfx.upload_rgba(f.w, f.h, &f.rgba)
}

impl App {
    pub fn new(paths: Paths, status: Box<dyn Status>, opts: Options, now: Millis) -> App {
        let settings = Settings::read(&paths);
        let theme = Theme::read(&paths);
        let favourites = Favourites::read(&paths);
        let state = State::read(&paths);
        let mut gfx = Renderer::new();
        let stage = Stage::MINI;
        let statics = Faces::new(&mut gfx, &paths, &theme, stage, opts.seed);
        let faces = CartFaces::spawn(paths.sd.clone());
        let carts = library::scan(&paths);
        let sound_given = opts.sound.is_some();
        let sound: Box<dyn crate::sound::Sound> = match opts.sound {
            Some(s) => s,
            None => Box::new(crate::sound::Player::onion(
                &paths.sd,
                paths.cache().join("sounds"),
            )),
        };
        let mut app = App {
            paths,
            settings,
            theme,
            favourites,
            carts,
            shelves: Vec::new(),
            at: 0,
            gfx,
            faces,
            face_wait: opts.face_wait,
            stage,
            statics,
            title: Printing::default(),
            clock: Printing::default(),
            percent: Printing::default(),
            shelf_named: None,
            notice: None,
            phase: Phase::Shelf,
            a_held: None,
            status,
            battery: None,
            wifi: false,
            polled: None,
            now,
            draws: Vec::new(),
            studio: None,
            about: None,
            sound,
            sound_given,
            fetched: None,
            fetch_started: false,
            restock_pending: false,
            exit: None,
        };
        app.restock();
        app.restore(&state);
        app
    }

    // ---- shelves -------------------------------------------------------

    fn restock(&mut self) {
        let keep = self.current().map(|(k, c)| (k, c.platform, c.stem.clone()));
        for (_, shelf) in self.shelves.drain(..) {
            let mut shelf = shelf;
            for i in 0..shelf.carts.len() {
                if let Some(t) = shelf.take_face(i) {
                    self.gfx.free(t);
                }
                if let Some(t) = shelf.take_interior(i) {
                    self.gfx.free(t);
                }
            }
        }
        self.faces.reset();
        self.shelves = library::shelves(&self.carts, &self.favourites)
            .into_iter()
            .map(|(kind, carts)| {
                let mut shelf = Shelf::new(carts);
                self.statics.dress(&mut shelf, &mut self.gfx, &self.theme);
                (kind, shelf)
            })
            .collect();
        self.mark_hearts();
        if let Some((kind, platform, stem)) = keep {
            self.select(kind, platform, &stem);
        }
        self.at = self.at.min(self.shelves.len().saturating_sub(1));
    }

    fn mark_hearts(&mut self) {
        for (_, shelf) in &mut self.shelves {
            let f = &self.favourites;
            shelf.mark_hearted(|c| f.contains(c.platform, &c.stem));
        }
    }

    /// Put the shelf of `kind` up with this cart selected, if both exist.
    fn select(&mut self, kind: ShelfKind, platform: Platform, stem: &str) -> bool {
        let Some(at) = self.shelves.iter().position(|(k, _)| *k == kind) else {
            return false;
        };
        self.at = at;
        let shelf = &mut self.shelves[at].1;
        if let Some(i) = shelf
            .carts
            .iter()
            .position(|c| c.platform == platform && c.stem == stem)
        {
            shelf.select(i);
            return true;
        }
        false
    }

    fn restore(&mut self, state: &State) {
        let kind = state.shelf.as_deref().and_then(|key| {
            self.shelves
                .iter()
                .map(|(k, _)| *k)
                .find(|k| k.key().eq_ignore_ascii_case(key))
        });
        if let Some((platform, stem)) = &state.cart {
            let found = kind.is_some_and(|k| self.select(k, *platform, stem))
                || self.select(ShelfKind::System(*platform), *platform, stem);
            if found && state.launched {
                // The eject starts with the first frame shown (`render`):
                // starting up can take longer than the whole animation.
                self.phase = Phase::Ejecting {
                    at: NOT_STARTED,
                    stem: stem.clone(),
                };
            }
        } else if let Some(k) = kind {
            if let Some(at) = self.shelves.iter().position(|(s, _)| *s == k) {
                self.at = at;
            }
        }
        if state.launched {
            // Clear the flag now: the eject plays once.
            let mut s = state.clone();
            s.launched = false;
            let _ = s.write(&self.paths);
        }
        self.shelf_named = Some(self.now);
    }

    pub fn state(&self) -> State {
        State {
            shelf: self.shelves.get(self.at).map(|(k, _)| k.key()),
            cart: self.current().map(|(_, c)| (c.platform, c.stem.clone())),
            launched: false,
        }
    }

    fn shelf(&self) -> Option<&Shelf> {
        self.shelves.get(self.at).map(|(_, s)| s)
    }

    fn shelf_mut(&mut self) -> Option<&mut Shelf> {
        self.shelves.get_mut(self.at).map(|(_, s)| s)
    }

    /// The selected cart and the shelf it is on.
    pub fn current(&self) -> Option<(ShelfKind, &Cart)> {
        let (kind, shelf) = self.shelves.get(self.at)?;
        Some((*kind, shelf.carts.get(shelf.index)?))
    }

    /// The key of the shelf on screen, for `state.txt`.
    pub fn shelf_key(&self) -> String {
        self.shelves
            .get(self.at)
            .map(|(k, _)| k.key())
            .unwrap_or_default()
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn shelf_kinds(&self) -> Vec<ShelfKind> {
        self.shelves.iter().map(|(k, _)| *k).collect()
    }

    fn switch_shelf(&mut self, by: i32) {
        let n = self.shelves.len() as i32;
        if n < 2 {
            return;
        }
        if let Some(s) = self.shelf_mut() {
            s.release_hold();
        }
        self.at = (self.at as i32 + by).rem_euclid(n) as usize;
        self.shelf_named = Some(self.now);
        self.notice = None;
    }

    fn toggle_favourite(&mut self) {
        let Some((kind, cart)) = self.current() else {
            return;
        };
        let (platform, stem) = (cart.platform, cart.stem.clone());
        self.favourites.toggle(platform, &stem);
        if let Err(e) = self.favourites.write(&self.paths) {
            eprintln!("slot-mini: favourites: {e}");
        }
        let fav_index = self.shelf().map(|s| s.index).unwrap_or(0);
        self.restock();
        match kind {
            // Un-hearting on the Favorites shelf: stay there on the next cart
            // if any are left, as slot does.
            ShelfKind::Favourites => {
                if let Some(at) = self
                    .shelves
                    .iter()
                    .position(|(k, _)| *k == ShelfKind::Favourites)
                {
                    self.at = at;
                    let shelf = &mut self.shelves[at].1;
                    shelf.select(fav_index.min(shelf.carts.len() - 1));
                } else {
                    self.select(ShelfKind::System(platform), platform, &stem);
                    self.shelf_named = Some(self.now);
                }
            }
            _ => {
                self.select(kind, platform, &stem);
            }
        }
    }

    // ---- input and time ------------------------------------------------

    pub fn handle(&mut self, ev: Event) {
        match ev {
            Event::Quit => self.exit = Some(Exit::Quit),
            Event::Press(b) => self.press(b),
            Event::Release(b) => self.release(b),
        }
    }

    fn press(&mut self, b: Button) {
        if self.phase == Phase::Studio {
            return self.studio_press(b);
        }
        if self.phase == Phase::About {
            return self.about_press(b);
        }
        if self.phase != Phase::Shelf {
            return;
        }
        let now = self.now;
        match b {
            Button::Left => self.shelf_mut().map_or((), |s| s.hold_left(now)),
            Button::Right => self.shelf_mut().map_or((), |s| s.hold_right(now)),
            Button::Up => self.shelf_mut().map_or((), |s| s.jump_prev_letter()),
            Button::Down => self.shelf_mut().map_or((), |s| s.jump_next_letter()),
            Button::L1 => self.switch_shelf(-1),
            Button::R1 => self.switch_shelf(1),
            Button::Y => self.toggle_favourite(),
            Button::A => self.a_held = Some(now),
            Button::Select => self.open_studio(),
            Button::Menu => self.open_about(),
            _ => {}
        }
    }

    fn release(&mut self, b: Button) {
        match b {
            Button::Left => self.shelf_mut().map_or((), |s| s.release_left()),
            Button::Right => self.shelf_mut().map_or((), |s| s.release_right()),
            Button::A if self.a_held.take().is_some() && self.phase == Phase::Shelf => {
                self.insert(false);
            }
            _ => {}
        }
    }

    fn open_studio(&mut self) {
        let Some((_, shelf)) = self.shelves.get(self.at) else {
            return;
        };
        let Some(cart) = shelf.carts.get(shelf.index).cloned() else {
            return;
        };
        let (at, index) = (self.at, shelf.index);
        if let Some(s) = self.shelf_mut() {
            s.release_hold();
        }
        self.a_held = None;
        self.studio = Some(crate::studio_ui::StudioUi::open(cart, at, index));
        self.phase = Phase::Studio;
    }

    fn open_about(&mut self) {
        if let Some(s) = self.shelf_mut() {
            s.release_hold();
        }
        self.a_held = None;
        self.about = Some(crate::about::About::new(&self.paths));
        self.phase = Phase::About;
    }

    fn about_press(&mut self, b: Button) {
        let Some(about) = self.about.as_mut() else {
            self.phase = Phase::Shelf;
            return;
        };
        match about.press(b, &mut self.settings, &self.paths) {
            crate::about::Outcome::Stay => {}
            crate::about::Outcome::Close => {
                if let Some(a) = self.about.take() {
                    a.close(&mut self.gfx);
                }
                self.phase = Phase::Shelf;
            }
            crate::about::Outcome::ExitToOnion => self.exit = Some(Exit::ToOnion),
        }
    }

    fn studio_press(&mut self, b: Button) {
        let Some(ui) = self.studio.as_mut() else {
            self.phase = Phase::Shelf;
            return;
        };
        let service = (self.settings.label_service, self.settings.art_host.as_str());
        if !ui.press(b, &self.paths, service, self.wifi, &mut self.gfx) {
            return;
        }
        // Closing: keep what changed, in slot's files.
        let mut ui = self.studio.take().unwrap();
        ui.save(&self.paths);
        let cart = ui.dressed();
        let before = self.carts.iter().find(|c| c.rom == cart.rom).cloned();
        let shell_changed = before.as_ref().is_some_and(|b| b.shell != cart.shell);
        if shell_changed {
            if let Err(e) = slot_mini_store::cart_shell::write_choice(
                &self.paths.sd,
                &cart.stem,
                cart.shell.as_ref(),
            ) {
                eprintln!("slot-mini: cart_shell.txt: {e}");
            }
        }
        let changed = shell_changed || ui.relabelled;
        ui.close(&mut self.gfx);
        self.phase = Phase::Shelf;
        if changed {
            for c in self
                .carts
                .iter_mut()
                .filter(|c| c.platform == cart.platform && c.stem == cart.stem)
            {
                c.shell = cart.shell;
                c.label = cart.label.clone();
            }
            self.restock();
        }
    }

    fn insert(&mut self, fresh: bool) {
        self.a_held = None;
        let Some((_, cart)) = self.current() else {
            return;
        };
        // Refuse before the cart goes in if Onion could not run it.
        let checked =
            crate::launch::record(&self.paths, cart).and_then(|r| crate::launch::command(&r));
        if let Err(e) = checked {
            eprintln!("slot-mini: {e}");
            self.notice = Some(match e {
                crate::launch::LaunchError::UnsafePath(_) => "Rename this file to launch it".into(),
                _ => "This game cannot be launched".into(),
            });
            self.shelf_named = Some(self.now);
            return;
        }
        let stem = cart.stem.clone();
        if let Some(s) = self.shelf_mut() {
            s.release_hold();
        }
        self.play(crate::sound::Clip::Insert);
        self.phase = Phase::Inserting {
            at: self.now,
            stem,
            fresh,
        };
    }

    /// Advance to `now` (milliseconds since start).
    pub fn tick(&mut self, now: Millis) {
        let dt = (now.saturating_sub(self.now)) as f32 / 1000.0;
        self.now = now;
        if let Some(at) = self.a_held {
            if self.phase == Phase::Shelf && now.saturating_sub(at) >= PLAY_HOLD_MS {
                self.insert(true);
            }
        }
        if let Some(s) = self.shelf_mut() {
            s.tick(now);
            s.update(dt.min(0.1));
        }
        if let Some(ui) = self.studio.as_mut() {
            ui.poll(&mut self.gfx);
        }
        self.sync_fetched();
        match &self.phase {
            Phase::Inserting { at, fresh, .. } if self.secs_since(*at) >= INSERT_S => {
                if let Some((_, cart)) = self.current() {
                    self.exit = Some(Exit::Launch {
                        cart: cart.clone(),
                        fresh: *fresh,
                    });
                }
            }
            Phase::Ejecting { at, .. } if self.secs_since(*at) >= EJECT_S => {
                self.phase = Phase::Shelf;
            }
            _ => {}
        }
        if self
            .polled
            .is_none_or(|p| now.saturating_sub(p) >= STATUS_POLL_MS)
        {
            self.polled = Some(now);
            self.battery = self.status.battery();
            self.wifi = self.status.wifi();
        }
    }

    /// slot's label service, when switched on and online: fetch labels for
    /// carts without one in the background. Local files always win.
    fn sync_fetched(&mut self) {
        if !self.fetch_started && self.settings.label_service && self.wifi {
            self.fetch_started = true;
            let (tx, rx) = std::sync::mpsc::channel();
            let root = self.paths.sd.clone();
            let base = crate::art::jobs::base_url(&self.settings.art_host);
            let carts: Vec<Cart> = self
                .carts
                .iter()
                .filter(|c| c.label.is_none())
                .cloned()
                .collect();
            let spawned = std::thread::Builder::new()
                .name("slot-mini-labels".into())
                .spawn(move || {
                    if let Err(e) = crate::art::jobs::fetch_missing(&root, &base, carts, &tx) {
                        eprintln!("slot-mini: label service: {e:?}");
                    }
                });
            if spawned.is_ok() {
                self.fetched = Some(rx);
            }
        }
        let Some(rx) = &self.fetched else {
            return;
        };
        let mut landed = Vec::new();
        while let Ok(got) = rx.try_recv() {
            landed.push(got);
        }
        let any = !landed.is_empty();
        for (cart, path) in landed {
            self.label_landed(&cart, path);
        }
        self.restock_pending |= any;
        // Rebuild faces only on the shelf, so an insert, eject or the studio
        // is never interrupted.
        if self.restock_pending && self.phase == Phase::Shelf {
            self.restock_pending = false;
            self.restock();
        }
    }

    fn label_landed(&mut self, cart: &Cart, path: std::path::PathBuf) {
        for c in self.carts.iter_mut().filter(|c| c.rom == cart.rom) {
            c.label = Some(path.clone());
        }
    }

    fn play(&mut self, clip: crate::sound::Clip) {
        if self.settings.sounds || self.sound_given {
            self.sound.play(clip);
        }
    }

    fn secs_since(&self, at: Millis) -> f32 {
        self.now.saturating_sub(at) as f32 / 1000.0
    }

    /// slot's `seat()`: how far the cart is into the slot.
    fn seat(&self) -> f32 {
        match &self.phase {
            Phase::Shelf => 0.0,
            Phase::Inserting { at, .. } => (self.secs_since(*at) / SEATED_AT).clamp(0.0, 1.0),
            Phase::Ejecting { at, .. } => 1.0 - (self.secs_since(*at) / EJECT_S).clamp(0.0, 1.0),
            Phase::Studio | Phase::About => 0.0,
        }
    }

    pub fn now(&self) -> Millis {
        self.now
    }

    /// How many carts in view are still drawn as their label colour.
    pub fn placeholders_on_screen(&self) -> usize {
        self.shelf()
            .map(|s| {
                s.on_screen()
                    .into_iter()
                    .filter(|&i| s.face(i).is_none())
                    .count()
            })
            .unwrap_or(0)
    }

    // ---- faces ---------------------------------------------------------

    /// Ask for the faces near the selection, upload the finished ones, and
    /// drop the ones far away.
    fn sync_faces(&mut self) {
        let Some((_, shelf)) = self.shelves.get(self.at) else {
            return;
        };
        let on_screen = shelf.on_screen();
        let wants: Vec<FaceWant> = shelf
            .around(NEAR)
            .into_iter()
            .filter(|&i| shelf.face(i).is_none())
            .map(|i| FaceWant {
                shelf: self.at,
                index: i,
                on_screen: on_screen.contains(&i),
            })
            .collect();
        let shelves = &self.shelves;
        let ready = self.faces.sync(
            wants,
            |s, i| shelves.get(s).and_then(|(_, sh)| sh.carts.get(i)),
            self.face_wait,
        );
        for r in ready {
            let tex = self.gfx.upload(r.face.full);
            self.gfx.pin_scaled(tex, r.face.side);
            let interior = r.interior.map(|f| {
                let t = self.gfx.upload(f.full);
                self.gfx.pin_scaled(t, f.side);
                t
            });
            let Some((_, shelf)) = self.shelves.get_mut(r.shelf) else {
                self.gfx.free(tex);
                continue;
            };
            if let Some(old) = shelf.set_face(r.index, tex) {
                self.gfx.free(old);
            }
            if let Some(t) = interior {
                if let Some(old) = shelf.set_interior(r.index, t) {
                    self.gfx.free(old);
                }
            }
        }
        // Keep memory flat: faces only near the selection on this shelf.
        for (s, (_, shelf)) in self.shelves.iter_mut().enumerate() {
            let near = if s == self.at {
                shelf.around(NEAR + 1)
            } else {
                Vec::new()
            };
            for i in 0..shelf.carts.len() {
                if near.contains(&i) {
                    continue;
                }
                if let Some(t) = shelf.take_face(i) {
                    self.gfx.free(t);
                }
                if let Some(t) = shelf.take_interior(i) {
                    self.gfx.free(t);
                }
            }
        }
    }

    /// Re-print text faces whose words changed.
    fn sync_text(&mut self) {
        let title = match &self.notice {
            Some(n) => n.clone(),
            None => self
                .shelves
                .get(self.at)
                .map(|(k, _)| k.title().to_string())
                .unwrap_or_default(),
        };
        reprint(&mut self.gfx, &mut self.title, &title, |t| {
            title_line_face(t)
        });
        let clock = clock_text(self.status.clock(), self.settings.clock_24h);
        let ink = self.theme.footer_ink;
        reprint(&mut self.gfx, &mut self.clock, &clock, |t| word_ink(t, ink));
        let pct = self.battery.map(|b| format!("{b}%")).unwrap_or_default();
        reprint(&mut self.gfx, &mut self.percent, &pct, |t| word_ink(t, ink));
    }

    /// slot's `slot_name_alpha`.
    fn title_alpha(&self) -> f32 {
        let Some(at) = self.shelf_named else {
            return 0.0;
        };
        let t = self.now.saturating_sub(at);
        let level = if t < SLOT_NAME_IN_MS {
            t as f32 / SLOT_NAME_IN_MS as f32
        } else if t < SLOT_NAME_IN_MS + SLOT_NAME_HOLD_MS {
            1.0
        } else {
            let out = t - SLOT_NAME_IN_MS - SLOT_NAME_HOLD_MS;
            1.0 - (out as f32 / SLOT_NAME_OUT_MS as f32).min(1.0)
        };
        ease(level)
    }

    // ---- drawing -------------------------------------------------------

    /// Bring faces and text up to date and draw the frame.
    pub fn render(&mut self, canvas: &mut Canvas) {
        self.sync_faces();
        self.sync_text();
        let mut out = std::mem::take(&mut self.draws);
        out.clear();
        match (self.studio.as_mut(), self.about.as_mut()) {
            (Some(ui), _) if self.phase == Phase::Studio => {
                ui.draw(&self.paths, self.stage, &mut self.gfx, &mut out)
            }
            (_, Some(about)) if self.phase == Phase::About => about.draw(
                self.stage,
                self.statics.ground,
                &self.settings,
                &mut self.gfx,
                &mut out,
            ),
            _ => self.draw(&mut out),
        }
        canvas.clear(0);
        self.gfx.render(&out, canvas);
        self.draws = out;
        if let Phase::Ejecting { at, .. } = &mut self.phase {
            if *at == NOT_STARTED {
                *at = self.now;
                self.play(crate::sound::Clip::Eject);
            }
        }
    }

    fn draw_background(&self, out: &mut Vec<Draw>) {
        draw_ground(self.stage, self.statics.ground, out);
        if self.theme.wallpaper_tint > 0.0 {
            out.push(Draw::Rect {
                x: 0.0,
                y: 0.0,
                w: self.stage.w,
                h: OUT_H as f32,
                colour: [0.0, 0.0, 0.0, self.theme.wallpaper_tint],
            });
        }
    }

    fn slot_faces(&self) -> Option<(TexId, TexId)> {
        Some(self.statics.slot[usize::from(self.theme.light)])
    }

    pub fn draw(&self, out: &mut Vec<Draw>) {
        let stage = self.stage;
        self.draw_background(out);
        let Some(shelf) = self.shelf() else {
            let (tex, w, h) = self.statics.empty;
            out.push(Draw::Tex {
                x: ((stage.w - w as f32) / 2.0).round(),
                y: ((slot_mini_ui::slot_name_y() - h as f32) / 2.0).round(),
                w: w as f32,
                h: h as f32,
                tex,
                alpha: 1.0,
            });
            slot_mini_ui::draw_empty_slot(stage, self.slot_faces(), out);
            self.draw_footer(out);
            return;
        };
        match &self.phase {
            Phase::Shelf => {
                shelf.draw(stage, 0.0, self.slot_faces(), out);
                draw_slot_title(stage, self.title.printed, self.title_alpha(), out);
            }
            // slot draws no footer while a cart goes in or comes out.
            Phase::Studio | Phase::About => {}
            Phase::Inserting { stem, .. } | Phase::Ejecting { stem, .. } => {
                shelf.draw_row(stage, Some(stem), 0.0, self.seat(), 1.0, out);
                return self.chrome(shelf, stem, out);
            }
        }
        self.draw_footer(out);
    }

    fn chrome(&self, shelf: &Shelf, stem: &str, out: &mut Vec<Draw>) {
        let Some((cart, face)) = shelf.find(stem) else {
            return;
        };
        let (rest, scale) = shelf.selected_at(self.stage);
        SlotChrome {
            cart,
            face,
            rest,
            scale,
            seat: self.seat(),
            alert: None,
            dim: 0.0,
            screen: 0.0,
            game: false,
            over: !matches!(self.phase, Phase::Ejecting { .. }),
            behind: shelf.behind(stem),
            slot: self.slot_faces(),
            canvas: self.stage,
        }
        .draw(out);
    }

    fn draw_footer(&self, out: &mut Vec<Draw>) {
        let battery = self.battery.map(|percent| Battery {
            percent,
            charge: Charge::Unknown,
        });
        draw_footer(
            self.stage,
            battery,
            self.percent.printed,
            None,
            self.clock.printed,
            self.theme.footer_ink,
            out,
        );
        if self.wifi {
            draw_wifi_icon(self.percent.printed, self.statics.wifi, out);
        }
    }
}

fn reprint(gfx: &mut Renderer, p: &mut Printing, text: &str, face: impl Fn(&str) -> UndoFace) {
    if p.text == text {
        return;
    }
    if let Some(t) = p.printed.face.take() {
        gfx.free(t);
    }
    p.text = text.to_string();
    p.printed = match text.is_empty() {
        true => Printed::default(),
        false => {
            let f = face(text);
            Printed::new(upload(gfx, &f), f.w)
        }
    };
}

impl Faces {
    fn new(gfx: &mut Renderer, paths: &Paths, theme: &Theme, stage: Stage, seed: u64) -> Faces {
        let heart = heart_face(HEART_SCALE, theme.heart_ink);
        let heart = Some((
            gfx.upload_rgba(heart.w, heart.h, &heart.rgba),
            heart.w,
            heart.h,
        ));
        let mut slot_pair = |back: &[u8], front: &[u8]| {
            (
                gfx.upload_rgba(slot_mini_ui::SLOT_W, slot_mini_ui::SLOT_H, back),
                gfx.upload_rgba(slot_mini_ui::SLOT_W, slot_mini_ui::SLOT_H, front),
            )
        };
        let slot = [
            slot_pair(slot_mini_ui::slot_back(), slot_mini_ui::slot_front()),
            slot_pair(
                slot_mini_ui::slot_back_light(),
                slot_mini_ui::slot_front_light(),
            ),
        ];
        let shadows = [
            upload_pinned(gfx, cart_shadow()),
            upload_pinned(gfx, gb_cart_shadow(GbShell::Notched)),
            upload_pinned(gfx, gb_cart_shadow(GbShell::Rounded)),
        ];
        let backings = [
            upload_pinned(gfx, cart_backing(None)),
            upload_pinned(gfx, cart_backing(Some(GbShell::Notched))),
            upload_pinned(gfx, cart_backing(Some(GbShell::Rounded))),
        ];
        let empty = title_line_face(EMPTY_TEXT);
        let empty = (upload(gfx, &empty), empty.w, empty.h);
        let wifi = slot_mini_ui::wifi_face(3, theme.footer_ink);
        let wifi = Some((upload(gfx, &wifi), wifi.w, wifi.h));
        let ground = ground(paths, stage, seed).map(|s| {
            let w = s.w;
            (gfx.upload(s), w)
        });
        Faces {
            heart,
            shadows,
            backings,
            empty,
            slot,
            ground,
            wifi,
        }
    }

    /// The textures every shelf shares: heart, shadows and see-through backings.
    fn dress(&self, shelf: &mut Shelf, _gfx: &mut Renderer, _theme: &Theme) {
        if let Some((t, w, h)) = self.heart {
            shelf.set_heart(t, w, h);
        }
        shelf.set_shadow(self.shadows[0]);
        shelf.set_gb_shadow(GbShell::Notched, self.shadows[1]);
        shelf.set_gb_shadow(GbShell::Rounded, self.shadows[2]);
        shelf.set_backing(self.backings[0]);
        shelf.set_gb_backing(GbShell::Notched, self.backings[1]);
        shelf.set_gb_backing(GbShell::Rounded, self.backings[2]);
    }
}

/// Upload a face with a copy pre-scaled to slot's side scale.
fn upload_pinned(gfx: &mut Renderer, f: slot_mini_ui::CartFace) -> TexId {
    let full = Sprite::from_rgba(f.w, f.h, &f.rgba);
    let side = full.resized(
        (f.w as f32 * slot_mini_ui::SIDE_SCALE).round() as u32,
        (f.h as f32 * slot_mini_ui::SIDE_SCALE).round() as u32,
    );
    let t = gfx.upload(full);
    gfx.pin_scaled(t, side);
    t
}

/// The wallpaper: a random PNG from `Wallpapers/` on the SD root, else
/// slot's default ground. Cropped to the screen once, here, so frames copy
/// it straight.
fn ground(paths: &Paths, stage: Stage, seed: u64) -> Option<Sprite> {
    let (w, h) = (stage.w as u32, OUT_H);
    let rgba = crate::wallpaper::pick(&paths.sd, seed)
        .and_then(|p| slot_mini_ui::wallpaper_face(&p, w, h))
        .or_else(|| {
            let full = slot_mini_ui::default_background_owned()?;
            Some(crop_centre(&full, slot_mini_gfx::OUT_W, OUT_H, w))
        })?;
    Some(Sprite::from_rgba(w, h, &rgba))
}

fn crop_centre(rgba: &[u8], sw: u32, sh: u32, w: u32) -> Vec<u8> {
    let x0 = ((sw - w) / 2) as usize;
    let mut out = Vec::with_capacity((w * sh * 4) as usize);
    for y in 0..sh as usize {
        let row = &rgba[(y * sw as usize + x0) * 4..][..(w * 4) as usize];
        out.extend_from_slice(row);
    }
    out
}
