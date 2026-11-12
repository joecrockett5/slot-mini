//! The on-device cart studio (SELECT): slot's `Studio` state machine and
//! `studio_view` layout. The resting cart is tilted once in software instead
//! of by slot's GL quad; the printer, its slips and the apply scene (peel,
//! toss, lift, stick, roll) follow slot's `draw_studio_sheet` and
//! `draw_apply_cart`, with the curling sticker drawn as software quads (see
//! docs/DESIGN-PARITY.md).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use slot_mini_gfx::{Button, Draw, Renderer, Sprite, Stage, TexId, OUT_H};
use slot_mini_store::{Cart, Platform};
use slot_mini_ui::{
    arrows_hint_face, board_in, cart_backing, cart_face_with, cart_interior, hint_face,
    shows_interior, title_line_face, word_ink, Placed, FOOTER_Y,
};

use crate::art::jobs::{self, Prints, UpdateFail};
use crate::art::make;
use crate::paths::Paths;
use crate::studio::{
    apply_stage, backing_shift, printer_light, slip_shuffle, slip_zoom, tray_step, ApplyScene,
    ApplyStage, Effect, LabelKind, Legend, Light, Screen, Slip, Studio, APPLY_MS, SLIP_SHUFFLE_MS,
};
use crate::studio_view::{
    layout, tray, turned, view, well_of, Align, Item, Style, CART_TILT, SLIP_PAD, TRAY_TOP,
};
use slot_mini_ui::curl_runs::joined;
use slot_mini_ui::sticker_curl::{strips, Pose};

/// What the label printer has to show.
pub enum Printer {
    /// Not opened yet this visit.
    Idle,
    /// The label service is off (the default).
    Disabled,
    NoWifi,
    Fetching(Receiver<Result<Prints, UpdateFail>>),
    Ready,
    Failed,
}

pub struct StudioUi {
    pub studio: Studio,
    /// The cart's shelf and index, to put it back after.
    pub shelf: usize,
    pub index: usize,
    cart_tex: Option<(TexId, u32, u32)>,
    built_for: Option<(
        Option<slot_mini_store::ShellChoice>,
        Option<PathBuf>,
        Option<[u8; 3]>,
    )>,
    pub printer: Printer,
    prints: HashMap<LabelKind, (Vec<u8>, TexId, u32, u32)>,
    text: HashMap<(String, Style), (TexId, u32, u32)>,
    chip_disc: Option<TexId>,
    /// Set when the label file changed, so the shelf rebuilds the face.
    pub relabelled: bool,
    /// The label the cart came in with, as art and as the file's bytes, read
    /// before anything is applied: its slip, and putting it back.
    existing: Option<Existing>,
    /// Each label kind as a sticker the size it is on the tray, and what it
    /// was built for (the title colour, the cart's well).
    stickers: HashMap<LabelKind, (TexId, Option<[u8; 3]>, [u32; 4])>,
    /// The tilted cart without its label, for the apply scene.
    bare_tex: Option<(TexId, u32, u32)>,
    /// The cart face's own size, to place the sticker on its well.
    face_wh: Option<(f32, f32)>,
    liner: Option<(TexId, u32, u32)>,
    printer_body: Option<TexId>,
    paper: Option<TexId>,
    /// slot's tray: the slips, how many were printed (the liner's travel),
    /// when the pick last moved, and the apply scene.
    now: f64,
    slips: Vec<Slip>,
    prints_made: u32,
    shuffled: f64,
    picked: Option<(LabelKind, f64)>,
    scene: Option<ApplyScene>,
}

const MAT: [f32; 4] = [0.122, 0.267, 0.216, 1.0];
const PAPER: [f32; 4] = [0.957, 0.945, 0.918, 1.0];

/// slot's `SLIP_SHUFFLE_PX`: how far piled slips pull apart on a pick.
/// The label the cart came in with.
struct Existing {
    art: Option<Vec<u8>>,
    file: Option<Vec<u8>>,
    /// It came without a label file.
    bare: bool,
}

const SLIP_SHUFFLE_PX: f32 = 28.0;
/// How far the liner under the printer moves before its pattern repeats
/// (slot scrolls its 121 px liner texture by 60 px).
const LINER_TILE: f32 = 60.0;
/// The liner is at least this tall, so the strip under the printer has a
/// full repeat to scroll through.
const LINER_MIN_H: u32 = 88;
/// slot's printer light colours.
const LIGHT_PRINTING: [f32; 4] = [0.37, 0.88, 0.48, 1.0];
const LIGHT_WAITING: [f32; 4] = [0.25, 0.55, 1.0, 1.0];
const LIGHT_OFF: [f32; 4] = [0.25, 0.25, 0.25, 1.0];

/// The message where the printed labels would be, when there are none.
pub const DISABLED_LINES: [&str; 2] = ["Make labels in Cart Studio", "studio.slot-cfw.fyi"];
pub const NO_WIFI_LINE: &str = "Label Printer needs Wi-Fi";

impl StudioUi {
    pub fn open(cart: Cart, shelf: usize, index: usize) -> StudioUi {
        StudioUi {
            // The printer row is always offered; without the service or
            // Wi-Fi it explains where labels come from instead.
            studio: Studio::new(cart, true),
            shelf,
            index,
            cart_tex: None,
            built_for: None,
            printer: Printer::Idle,
            prints: HashMap::new(),
            text: HashMap::new(),
            chip_disc: None,
            relabelled: false,
            existing: None,
            stickers: HashMap::new(),
            bare_tex: None,
            face_wh: None,
            liner: None,
            printer_body: None,
            paper: None,
            now: 0.0,
            slips: Vec::new(),
            prints_made: 0,
            shuffled: f64::NEG_INFINITY,
            picked: None,
            scene: None,
        }
    }

    /// Whether the apply scene is playing (input waits for it).
    pub fn applying(&self) -> bool {
        self.scene.is_some()
    }

    /// Slips on the tray, printed or printing.
    pub fn slip_count(&self) -> usize {
        self.slips.len()
    }

    /// The cart as the studio currently dresses it.
    pub fn dressed(&self) -> Cart {
        let mut c = self.studio.cart.clone();
        c.shell = self.studio.shell;
        c
    }

    /// Handle a button; `Some(true)` closes the studio.
    pub fn press(
        &mut self,
        b: Button,
        paths: &Paths,
        service: (bool, &str),
        wifi: bool,
        gfx: &mut Renderer,
    ) -> bool {
        // slot's `studio_input`: nothing while a label goes on, and A only
        // takes a slip that has come to rest.
        if self.scene.is_some() {
            return false;
        }
        let now = self.now;
        let was = self.studio.wearing;
        let used_y = match self.studio.screen {
            Screen::Sheet { at } => {
                let picked = self.studio.sheet.get(at).copied();
                let slip = self.slips.iter().find(|s| Some(s.kind) == picked);
                let resting = slip.filter(|s| !s.moving(now));
                if b == Button::A && resting.is_none() {
                    return false;
                }
                resting.map_or(0.0, |s| s.y1)
            }
            _ => 0.0,
        };
        let from = self.studio.screen;
        let effect = self.studio.press(b);
        if let (Screen::Sheet { at: a }, Screen::Sheet { at: b }) = (from, self.studio.screen) {
            if a != b && effect.is_none() {
                self.shuffled = now;
            }
        }
        match effect {
            Some(Effect::Close) => return true,
            Some(Effect::Fetch) => self.fetch(paths, service, wifi),
            Some(Effect::Apply(kind)) if self.apply(kind, paths, gfx) => {
                self.scene = Some(ApplyScene {
                    chosen: kind,
                    was,
                    used_y,
                    started: now,
                });
            }
            Some(Effect::Apply(_)) | None => {}
        }
        false
    }

    /// Advance the tray and the apply scene to `now` (slot's `tick_tray`
    /// and the end of its apply scene).
    pub fn tick(&mut self, now: u64, stage: Stage) {
        let now = now as f64;
        self.now = now;
        if self
            .scene
            .as_ref()
            .is_some_and(|s| now - s.started >= APPLY_MS)
        {
            self.scene = None;
        }
        if self.scene.is_some() {
            return;
        }
        let Screen::Sheet { at } = self.studio.screen else {
            self.slips.clear();
            return;
        };
        let picked = self.studio.sheet.get(at).copied();
        if picked != self.picked.map(|(kind, _)| kind) {
            self.picked = picked.map(|kind| (kind, now));
        }
        let t = tray(stage.w, &self.studio.cart);
        let ready: Vec<LabelKind> = [
            LabelKind::Existing,
            LabelKind::Real,
            LabelKind::Logo,
            LabelKind::Title,
        ]
        .into_iter()
        .filter(|k| self.has_face(*k))
        .collect();
        let before = self.slips.len();
        tray_step(
            &mut self.slips,
            &self.studio.sheet,
            &|k| ready.contains(&k),
            now,
            t.slip_h,
            t.bottom,
        );
        if self.slips.len() > before {
            self.prints_made += 1;
        }
    }

    /// slot's `studio_face(kind).is_some()`: printed labels once they have
    /// arrived; the cart's own and the text label always.
    fn has_face(&self, kind: LabelKind) -> bool {
        match kind {
            LabelKind::Real | LabelKind::Logo => self.prints.contains_key(&kind),
            LabelKind::Existing | LabelKind::Title => true,
        }
    }

    fn fetch(&mut self, paths: &Paths, (enabled, host): (bool, &str), wifi: bool) {
        if !matches!(self.printer, Printer::Idle | Printer::Failed) {
            return;
        }
        if !enabled {
            self.printer = Printer::Disabled;
            return;
        }
        if !wifi {
            self.printer = Printer::NoWifi;
            return;
        }
        let (tx, rx) = mpsc::channel();
        let (root, base, cart) = (
            paths.sd.clone(),
            jobs::base_url(host),
            self.studio.cart.clone(),
        );
        let spawned = std::thread::Builder::new()
            .name("slot-mini-print".into())
            .spawn(move || {
                let _ = tx.send(jobs::studio_prints(&root, &base, &cart));
            });
        self.printer = match spawned {
            Ok(_) => Printer::Fetching(rx),
            Err(_) => Printer::Failed,
        };
    }

    /// Collect printed labels when the fetch finishes.
    pub fn poll(&mut self, gfx: &mut Renderer) {
        let Printer::Fetching(rx) = &self.printer else {
            return;
        };
        let Ok(result) = rx.try_recv() else {
            return;
        };
        match result {
            Ok(prints) => {
                for (kind, png) in [
                    (LabelKind::Real, prints.real),
                    (LabelKind::Logo, prints.logo),
                ] {
                    if let Some(png) = png {
                        if let Some((rgba, w, h)) = slot_mini_ui::decode_png_bytes(&png) {
                            let t = gfx.upload_rgba(w, h, &rgba);
                            self.prints.insert(kind, (png, t, w, h));
                        }
                    }
                }
                let (real, logo) = (
                    self.prints.contains_key(&LabelKind::Real),
                    self.prints.contains_key(&LabelKind::Logo),
                );
                self.studio.art_arrived(real, logo);
                self.printer = Printer::Ready;
            }
            Err(_) => {
                self.studio.art_failed = true;
                self.studio.art_arrived(false, false);
                self.printer = Printer::Failed;
            }
        }
    }

    fn apply(&mut self, kind: LabelKind, paths: &Paths, gfx: &mut Renderer) -> bool {
        self.remember_existing(paths);
        let cart = self.studio.cart.clone();
        let png = match kind {
            LabelKind::Real | LabelKind::Logo => self.prints.get(&kind).map(|p| p.0.clone()),
            LabelKind::Title => {
                let colour = self
                    .studio
                    .ground
                    .unwrap_or(slot_mini_ui::label_colour(&slot_mini_ui::label_text(&cart)));
                Some(make::text_label(&cart, colour))
            }
            // The cart's own label back; a cart that came without one goes
            // back to none.
            LabelKind::Existing => self.existing.as_ref().and_then(|e| e.file.clone()),
        };
        let bare = self.existing.as_ref().is_some_and(|e| e.bare);
        let written = match (&png, kind) {
            (Some(png), _) => make::relabel(&paths.sd, &cart, png),
            (None, LabelKind::Existing) if bare => make::apply_label(&paths.sd, &cart, None),
            (None, _) => return false,
        };
        if let Err(e) = written {
            eprintln!("slot-mini: studio: label: {e}");
            return false;
        }
        self.studio.cart.label = png.map(|_| make::label_path(&paths.sd, &cart));
        self.studio.applied(kind);
        self.relabelled = true;
        self.invalidate(gfx);
        true
    }

    /// Read the cart's own label once, before anything replaces it.
    fn remember_existing(&mut self, paths: &Paths) {
        if self.existing.is_some() {
            return;
        }
        let cart = &self.studio.cart;
        let art = crate::label_cache::label_art(&paths.sd, cart);
        let file = cart.label.as_ref().and_then(|p| std::fs::read(p).ok());
        self.existing = Some(Existing {
            art,
            file,
            bare: cart.label.is_none(),
        });
    }

    fn invalidate(&mut self, gfx: &mut Renderer) {
        for (t, ..) in [self.cart_tex.take(), self.bare_tex.take()]
            .into_iter()
            .flatten()
        {
            gfx.free(t);
        }
        self.built_for = None;
    }

    /// On close: a recoloured text label is kept as a label file, as slot's
    /// studio does, so Cart Studio and the shelf both see it.
    pub fn save(&mut self, paths: &Paths) {
        if !(self.studio.recolored() && self.studio.wearing_text()) {
            return;
        }
        let Some(g) = self.studio.ground else {
            return;
        };
        let cart = self.studio.cart.clone();
        match make::relabel(&paths.sd, &cart, &make::text_label(&cart, g)) {
            Ok(()) => {
                self.studio.cart.label = Some(make::label_path(&paths.sd, &cart));
                self.relabelled = true;
            }
            Err(e) => eprintln!("slot-mini: studio: label: {e}"),
        }
    }

    /// Free every texture the studio made.
    pub fn close(mut self, gfx: &mut Renderer) {
        self.invalidate(gfx);
        for (_, (_, t, ..)) in self.prints.drain() {
            gfx.free(t);
        }
        for (_, (t, ..)) in self.text.drain() {
            gfx.free(t);
        }
        for (_, (t, ..)) in self.stickers.drain() {
            gfx.free(t);
        }
        let liner = self.liner.take().map(|l| l.0);
        for t in [
            self.chip_disc.take(),
            self.printer_body.take(),
            self.paper.take(),
            liner,
        ]
        .into_iter()
        .flatten()
        {
            gfx.free(t);
        }
    }

    /// The tilted cart, rebuilt when the shell or label changes.
    fn cart_face(
        &mut self,
        paths: &Paths,
        stage: Stage,
        gfx: &mut Renderer,
    ) -> Option<(TexId, u32, u32)> {
        let cart = self.dressed();
        let ground = self
            .studio
            .preview_ground()
            .filter(|_| self.studio.wearing_text());
        let key = (cart.shell, cart.label.clone(), ground);
        if self.built_for.as_ref() == Some(&key) {
            return self.cart_tex;
        }
        self.invalidate(gfx);
        let art = match ground {
            // A text label being recoloured: slot's generated label on the
            // chosen ground.
            Some(g) => Some(slot_mini_ui::text_label(&cart, g)),
            None => crate::label_cache::label_art(&paths.sd, &cart),
        };
        let face = cart_face_with(&cart, art);
        let turned = tilted(&cart, stage, &face);
        let (w, h) = (turned.w, turned.h);
        self.cart_tex = Some((gfx.upload(turned), w, h));
        self.built_for = Some(key);
        self.cart_tex
    }

    /// The tilted cart without a label, under the apply scene's sticker.
    fn bare_face(&mut self, stage: Stage, gfx: &mut Renderer) -> Option<(TexId, u32, u32)> {
        if self.bare_tex.is_none() {
            let cart = self.dressed();
            let face = slot_mini_ui::bare_face(&cart);
            self.face_wh = Some((face.w as f32, face.h as f32));
            let turned = tilted(&cart, stage, &face);
            let (w, h) = (turned.w, turned.h);
            self.bare_tex = Some((gfx.upload(turned), w, h));
        }
        self.bare_tex
    }

    /// `kind` as a sticker: the cart's label well cut from its face, at the
    /// size it has on the tray (slot draws the face texture's well).
    fn sticker(&mut self, kind: LabelKind, stage: Stage, gfx: &mut Renderer) -> Option<TexId> {
        let cart = self.dressed();
        let well = well_of(&cart);
        let title = (kind == LabelKind::Title).then(|| {
            self.studio
                .ground
                .unwrap_or(slot_mini_ui::label_colour(&slot_mini_ui::label_text(&cart)))
        });
        if let Some((t, c, w)) = self.stickers.get(&kind) {
            if *c == title && *w == well {
                return Some(*t);
            }
        }
        let (lw, lh) = slot_mini_ui::label_size(&cart);
        let art = match kind {
            LabelKind::Real | LabelKind::Logo => {
                let png = &self.prints.get(&kind)?.0;
                Some(slot_mini_ui::cover_bytes(png, lw, lh)?)
            }
            LabelKind::Title => title.map(|c| slot_mini_ui::text_label(&cart, c)),
            // A cart without art wears the generated label.
            LabelKind::Existing => self.existing.as_ref().and_then(|e| e.art.clone()),
        };
        let face = cart_face_with(&cart, art);
        let t = tray(stage.w, &cart);
        let crop = crop(&face, well).resized(
            t.sticker_w.round().max(1.0) as u32,
            t.sticker_h.round().max(1.0) as u32,
        );
        let tex = gfx.upload(crop);
        if let Some((old, ..)) = self.stickers.insert(kind, (tex, title, well)) {
            gfx.free(old);
        }
        Some(tex)
    }

    fn text(&mut self, gfx: &mut Renderer, s: &str, style: Style) -> (TexId, u32, u32) {
        let key = (s.to_string(), style);
        if let Some(t) = self.text.get(&key) {
            return *t;
        }
        let face = match style {
            Style::Heading => title_line_face(s),
            Style::Detail | Style::Digit => word_ink(&s.to_uppercase(), [0xf6, 0xf4, 0xef]),
            Style::DigitOn => word_ink(&s.to_uppercase(), [0x1a, 0x19, 0x17]),
        };
        let t = (gfx.upload_rgba(face.w, face.h, &face.rgba), face.w, face.h);
        self.text.insert(key, t);
        t
    }

    fn disc(&mut self, gfx: &mut Renderer) -> TexId {
        if let Some(t) = self.chip_disc {
            return t;
        }
        let n = 32u32;
        let mut rgba = vec![0u8; (n * n * 4) as usize];
        for y in 0..n {
            for x in 0..n {
                let d = ((x as f32 + 0.5 - n as f32 / 2.0).powi(2)
                    + (y as f32 + 0.5 - n as f32 / 2.0).powi(2))
                .sqrt();
                let a = ((n as f32 / 2.0 - d) * 2.0).clamp(0.0, 1.0);
                let i = ((y * n + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[255, 255, 255, (a * 255.0) as u8]);
            }
        }
        let t = gfx.upload_rgba(n, n, &rgba);
        self.chip_disc = Some(t);
        t
    }

    pub fn draw(&mut self, paths: &Paths, stage: Stage, gfx: &mut Renderer, out: &mut Vec<Draw>) {
        self.remember_existing(paths);
        ground(stage, out);
        let l = layout(stage.w, self.studio.cart.platform);
        let shown = view(&self.studio, stage.w);
        for item in shown.items {
            match item {
                Item::Cart => {
                    let face = match self.scene {
                        Some(_) => self.bare_face(stage, gfx),
                        None => self.cart_face(paths, stage, gfx),
                    };
                    if let Some((tex, w, h)) = face {
                        let (cx, cy) = (l.cart_x + l.cart_w / 2.0, l.cart_y + l.cart_h / 2.0);
                        out.push(Draw::Tex {
                            x: (cx - w as f32 / 2.0).round(),
                            y: (cy - h as f32 / 2.0).round(),
                            w: w as f32,
                            h: h as f32,
                            tex,
                            alpha: 1.0,
                        });
                    }
                }
                Item::Text {
                    text,
                    style,
                    align,
                    x,
                    y,
                    alpha,
                    scale,
                } => {
                    let (tex, w, h) = self.text(gfx, &text, style);
                    let (w, h) = (w as f32 * scale, h as f32 * scale);
                    let x = match align {
                        Align::Left => x,
                        Align::Center => x - w / 2.0,
                        Align::Right => x - w,
                    };
                    out.push(Draw::Tex {
                        x: x.floor(),
                        y,
                        w,
                        h,
                        tex,
                        alpha,
                    });
                }
                Item::Rect { x, y, w, h, color } => out.push(Draw::Rect {
                    x,
                    y,
                    w,
                    h,
                    colour: color,
                }),
                Item::Chip {
                    cell,
                    x,
                    y,
                    selected,
                    ..
                } => self.chip(gfx, cell, x, y, selected, out),
                Item::Dot { x, y, size, alpha } => {
                    let tex = self.disc(gfx);
                    out.push(Draw::Tex {
                        x,
                        y,
                        w: size,
                        h: size,
                        tex,
                        alpha,
                    });
                }
                Item::Sheet => self.sheet(stage, gfx, out),
            }
        }
        if let Screen::Sheet { .. } = self.studio.screen {
            self.printer_message(stage, &l, gfx, out);
        }
        slot_mini_ui::draw_empty_slot(stage, None, out);
        if self.scene.is_some() {
            self.apply_cart(stage, gfx, out);
        }
        self.legends(stage, gfx, out);
    }

    fn chip(
        &mut self,
        gfx: &mut Renderer,
        cell: u32,
        x: f32,
        y: f32,
        selected: bool,
        out: &mut Vec<Draw>,
    ) {
        const SIZE: f32 = 48.0;
        let presets = slot_mini_ui::shell_presets();
        let colour = match cell as usize {
            i if i < presets.len() => Some(presets[i].1.colour),
            i if i == presets.len() => Some(
                self.studio
                    .cart
                    .shell
                    .map_or(self.studio.current_shell().colour, |c| c.colour),
            ),
            _ => None,
        };
        let disc = self.disc(gfx);
        if selected {
            out.push(Draw::Tex {
                x: x - 6.0,
                y: y - 6.0,
                w: SIZE + 12.0,
                h: SIZE + 12.0,
                tex: disc,
                alpha: 1.0,
            });
        }
        let rgb = colour.unwrap_or([0x55, 0x5a, 0x58]);
        let c = rgb.map(|v| v as f32 / 255.0);
        // A filled disc: the white disc, then the colour drawn over it as
        // concentric rects would be blocky, so tint by layering a dark rim.
        out.push(Draw::Tex {
            x: x - 1.0,
            y: y - 1.0,
            w: SIZE + 2.0,
            h: SIZE + 2.0,
            tex: disc,
            alpha: 0.25,
        });
        out.push(Draw::Rect {
            x: x + 8.0,
            y: y + 8.0,
            w: SIZE - 16.0,
            h: SIZE - 16.0,
            colour: [c[0], c[1], c[2], 1.0],
        });
        out.push(Draw::Rect {
            x: x + 4.0,
            y: y + 12.0,
            w: SIZE - 8.0,
            h: SIZE - 24.0,
            colour: [c[0], c[1], c[2], 1.0],
        });
        out.push(Draw::Rect {
            x: x + 12.0,
            y: y + 4.0,
            w: SIZE - 24.0,
            h: SIZE - 8.0,
            colour: [c[0], c[1], c[2], 1.0],
        });
    }

    /// slot's `draw_slip`: a liner slip with its sticker, `zoom`ed about
    /// its centre.
    #[allow(clippy::too_many_arguments)]
    fn slip(
        &mut self,
        kind: Option<LabelKind>,
        y: f32,
        shift: f32,
        zoom: f32,
        stage: Stage,
        gfx: &mut Renderer,
        out: &mut Vec<Draw>,
    ) {
        let t = tray(stage.w, &self.studio.cart);
        let (w, h) = (t.slip_w * zoom, t.slip_h * zoom);
        let (x, y) = (t.x + shift - (w - t.slip_w) / 2.0, y - (h - t.slip_h) / 2.0);
        out.push(Draw::Rect {
            x: x + 1.0,
            y: y + 3.0,
            w,
            h,
            colour: [0.0, 0.0, 0.0, 0.4],
        });
        let (liner, lw, lh) = self.liner(stage, gfx);
        out.push(Draw::Part {
            x,
            y,
            w,
            h,
            tex: liner,
            alpha: 1.0,
            uv: [0.0, 0.0, t.slip_w / lw as f32, t.slip_h / lh as f32],
        });
        let Some(face) = kind.and_then(|k| self.sticker(k, stage, gfx)) else {
            return;
        };
        let (sw, sh) = match gfx.get(face) {
            Some(s) if zoom == 1.0 => (s.w as f32, s.h as f32),
            _ => (t.sticker_w * zoom, t.sticker_h * zoom),
        };
        out.push(Draw::Tex {
            x: x + SLIP_PAD * zoom,
            y: y + SLIP_PAD * zoom,
            w: sw,
            h: sh,
            tex: face,
            alpha: 1.0,
        });
    }

    /// The slip backing, the size of a slip: slot's `liner_face`.
    fn liner(&mut self, stage: Stage, gfx: &mut Renderer) -> (TexId, u32, u32) {
        if let Some(l) = self.liner {
            return l;
        }
        let t = tray(stage.w, &self.studio.cart);
        let (w, h) = (
            t.slip_w.round() as u32,
            (t.slip_h.round() as u32).max(LINER_MIN_H),
        );
        let face = slot_mini_ui::liner::liner_face(w, h);
        let l = (gfx.upload_rgba(face.w, face.h, &face.rgba), w, h);
        self.liner = Some(l);
        l
    }

    /// slot's `draw_studio_sheet`: the slips on the tray, the picked one
    /// held up, the liner feeding out of the printer, and its light.
    fn sheet(&mut self, stage: Stage, gfx: &mut Renderer, out: &mut Vec<Draw>) {
        let t = tray(stage.w, &self.studio.cart);
        let now = self.now;
        let picked = match self.studio.screen {
            Screen::Sheet { at } => self.studio.sheet.get(at).copied(),
            _ => None,
        };
        let scene = self.scene.clone();
        let used = scene.as_ref().map(|s| s.chosen);
        let held = |s: &Slip| Some(s.kind) == picked && !s.moving(now) && scene.is_none();
        let slips: Vec<Slip> = self
            .slips
            .iter()
            .filter(|s| Some(s.kind) != used)
            .copied()
            .collect();
        let piled = t.bottom < t.slip_h;
        let since = self.picked.map_or(now, |(_, at)| at);
        let apart = match piled {
            true => slip_shuffle(now - self.shuffled) * SLIP_SHUFFLE_PX,
            false => 0.0,
        };
        let dim = scene.is_some() || slips.iter().any(held);
        let rest = |ui: &mut Self, gfx: &mut Renderer, out: &mut Vec<Draw>| {
            for slip in slips.iter().filter(|s| !held(s)) {
                let y = TRAY_TOP + slip.y(now);
                ui.slip(Some(slip.kind), y, apart, 1.0, stage, gfx, out);
                if dim && !slip.moving(now) {
                    out.push(Draw::Rect {
                        x: t.x + apart,
                        y,
                        w: t.slip_w,
                        h: t.slip_h,
                        colour: [0.0, 0.0, 0.0, 0.5],
                    });
                }
            }
        };
        let swapped = !piled || now - self.shuffled >= SLIP_SHUFFLE_MS / 2.0;
        if swapped {
            rest(self, gfx, out);
        }
        for slip in slips.iter().filter(|s| held(s)) {
            let zoom = if piled { 1.0 } else { slip_zoom(now - since) };
            let (w, h) = (t.slip_w * zoom, t.slip_h * zoom);
            let x = t.x - apart - (w - t.slip_w) / 2.0;
            let y = TRAY_TOP + slip.y(now) - (h - t.slip_h) / 2.0;
            for (grow, colour) in [(6.0, [0.0, 0.0, 0.0, 0.85]), (4.0, [1.0, 1.0, 1.0, 1.0])] {
                out.push(Draw::Rect {
                    x: x - grow,
                    y: y - grow,
                    w: w + 2.0 * grow,
                    h: h + 2.0 * grow,
                    colour,
                });
            }
            self.slip(
                Some(slip.kind),
                TRAY_TOP + slip.y(now),
                -apart,
                zoom,
                stage,
                gfx,
                out,
            );
        }
        if !swapped {
            rest(self, gfx, out);
        }
        if let Some(scene) = &scene {
            let (stage_now, p) = apply_stage(now - scene.started);
            let lifted = !matches!(stage_now, ApplyStage::Peel | ApplyStage::Toss);
            let kind = (!lifted).then_some(scene.chosen);
            let shift = match stage_now {
                ApplyStage::Roll => backing_shift(p, stage.w - t.x),
                _ => 0.0,
            };
            self.slip(kind, TRAY_TOP + scene.used_y, shift, 1.0, stage, gfx, out);
        }
        // The liner feeding out under the printer, moved on by each print.
        let printing = self.slips.iter().find(|s| s.printing(now));
        let travel = self.prints_made as f32 * t.slip_h + printing.map_or(0.0, |s| s.y(now) - s.y1);
        let (liner, lw, lh) = self.liner(stage, gfx);
        let top = lh as f32 - LINER_TILE - travel.rem_euclid(LINER_TILE);
        out.push(Draw::Part {
            x: t.x,
            y: 0.0,
            w: t.slip_w,
            h: 24.0,
            tex: liner,
            alpha: 1.0,
            uv: [0.0, top / lh as f32, t.slip_w / lw as f32, 24.0 / lh as f32],
        });
        let (px, pw) = (t.printer_x, t.printer_w);
        out.push(Draw::Rect {
            x: px,
            y: 20.0,
            w: pw,
            h: 34.0,
            colour: [0.0, 0.0, 0.0, 0.45],
        });
        let body = *self.printer_body.get_or_insert_with(|| {
            let (w, h) = (slot_mini_ui::SLOT_W, slot_mini_ui::SLOT_H);
            gfx.upload_rgba(w, h, slot_mini_ui::slot_plugged_light())
        });
        if let Some(b) = gfx.get(body) {
            let (bw, bh) = (b.w as f32, b.h as f32);
            out.push(Draw::Part {
                x: px,
                y: 16.0,
                w: pw,
                h: 32.0,
                tex: body,
                alpha: 1.0,
                uv: [0.1, 0.1, pw / bw, 32.0 / bh],
            });
        }
        let waiting = matches!(self.printer, Printer::Fetching(_));
        let lit = self.slips.iter().any(|s| s.lit(now));
        let colour = match printer_light(lit, waiting, now) {
            Light::Printing => LIGHT_PRINTING,
            Light::Waiting => LIGHT_WAITING,
            Light::Off => LIGHT_OFF,
        };
        out.push(Draw::Rect {
            x: px + pw - 18.0,
            y: 28.0,
            w: 7.0,
            h: 7.0,
            colour,
        });
    }

    /// slot's `draw_apply_cart` (the top half; the bare cart is drawn in
    /// the cart's place): the old label peels off and is tossed, the new
    /// one lifts from its slip, flies to the cart and rolls on.
    fn apply_cart(&mut self, stage: Stage, gfx: &mut Renderer, out: &mut Vec<Draw>) {
        let Some(scene) = self.scene.clone() else {
            return;
        };
        let cart = self.dressed();
        let l = layout(stage.w, cart.platform);
        let (x, y, w, h) = (l.cart_x, l.cart_y, l.cart_w, l.cart_h);
        let well = well_of(&cart);
        let Some((fw, fh)) = self.face_wh else {
            return;
        };
        let (sx, sy) = (w / fw, h / fh);
        let rect = [
            x + well[0] as f32 * sx,
            y + well[1] as f32 * sy,
            well[2] as f32 * sx,
            well[3] as f32 * sy,
        ];
        let t = tray(stage.w, &cart);
        let spot = [t.x + SLIP_PAD, TRAY_TOP + scene.used_y + SLIP_PAD];
        let centre = [x + w / 2.0, y + h / 2.0];
        let tilt = CART_TILT;
        let seat = turned(rect, centre, tilt)[0];
        let (dx, dy) = (seat[0] - rect[0], seat[1] - rect[1]);
        let seated = Pose {
            x: dx,
            y: dy,
            z: 0.0,
            turn_deg: tilt,
            scale: 1.0,
        };
        let pose_at = |p: f32| {
            let arc = (p * std::f32::consts::PI).sin();
            Pose {
                x: dx + (spot[0] - seat[0]) * p,
                y: dy + (spot[1] - seat[1]) * p - 30.0 * arc,
                z: 90.0 * arc,
                turn_deg: tilt * (1.0 - p),
                scale: 1.0 + (t.sticker_w / rect[2] - 1.0) * p,
            }
        };
        let (stage_now, p) = apply_stage(self.now - scene.started);
        let (Some(old), Some(new)) = (
            self.sticker(scene.was, stage, gfx),
            self.sticker(scene.chosen, stage, gfx),
        ) else {
            return;
        };
        let paper = *self.paper.get_or_insert_with(|| {
            let [r, g, b, _] = PAPER.map(|c| (c * 255.0).round() as u8);
            gfx.upload_rgba(1, 1, &[r, g, b, 255])
        });
        let focus = [stage.w / 2.0, 240.0];
        let stick = |tex: TexId, flat: f32, curl: f32, pose: Pose, out: &mut Vec<Draw>| {
            for strip in joined(strips(rect, flat, curl, &pose, focus)) {
                let (tex, uv) = match strip.back {
                    true => (paper, [0.0, 0.0, 1.0, 1.0]),
                    false => (tex, strip.uv),
                };
                out.push(Draw::Quad {
                    corners: strip.corners,
                    tex,
                    uv,
                    alpha: 1.0,
                });
            }
        };
        match stage_now {
            ApplyStage::Peel => {
                let q = p * p;
                stick(old, 1.0 - q, 150.0 + 270.0 * q, seated, out);
            }
            ApplyStage::Toss => {
                let (q, arc) = (p * p, (p * std::f32::consts::PI).sin());
                let pose = Pose {
                    x: dx - 520.0 * q,
                    y: dy - 60.0 * q - 50.0 * arc,
                    z: 90.0 * arc,
                    turn_deg: tilt - 24.0 * q,
                    scale: 1.0,
                };
                stick(old, 0.0, 420.0, pose, out);
            }
            ApplyStage::Lift => stick(new, 1.0 - p, 150.0, pose_at(1.0), out),
            ApplyStage::ToCart => stick(new, 0.0, 150.0, pose_at(1.0 - p), out),
            ApplyStage::Roll => stick(new, p, 150.0, seated, out),
        }
    }

    fn printer_message(
        &mut self,
        stage: Stage,
        l: &crate::studio_view::Layout,
        gfx: &mut Renderer,
        out: &mut Vec<Draw>,
    ) {
        let lines: &[&str] = match self.printer {
            Printer::Disabled => &DISABLED_LINES,
            Printer::NoWifi => &[NO_WIFI_LINE],
            Printer::Fetching(_) => &["Printing..."],
            _ => &[],
        };
        let mid = (l.edge + stage.w - l.margin) / 2.0;
        let top = 180.0 - lines.len() as f32 * 14.0;
        for (i, line) in lines.iter().enumerate() {
            let (tex, w, h) = self.text(gfx, line, Style::Detail);
            out.push(Draw::Tex {
                x: (mid - w as f32 / 2.0).floor(),
                y: top + i as f32 * 32.0,
                w: w as f32,
                h: h as f32,
                tex,
                alpha: 1.0,
            });
        }
    }

    fn legends(&mut self, stage: Stage, gfx: &mut Renderer, out: &mut Vec<Draw>) {
        let mut legends = self.studio.legends();
        // slot's `draw_studio_legends`: APPLY only with the picked slip at
        // rest, and not while a label goes on.
        if let Screen::Sheet { at } = self.studio.screen {
            let picked = self.studio.sheet.get(at).copied();
            let now = self.now;
            let ready = self
                .slips
                .iter()
                .any(|s| Some(s.kind) == picked && !s.moving(now));
            if !ready || self.scene.is_some() {
                legends.retain(|l| *l != Legend::Apply);
            }
        }
        let hint = |l: &Legend| -> (&'static str, &'static str) {
            match l {
                Legend::Back => ("B", "BACK"),
                Legend::Open => ("A", "OPEN"),
                Legend::Apply => ("A", "APPLY"),
                Legend::Change => ("LR", "CHANGE"),
                Legend::ChangeVertical => ("UD", "CHANGE"),
                Legend::Move => ("LR", "MOVE"),
                Legend::Reset => ("X", "RESET"),
                Legend::Hex => ("Y", "HEX"),
            }
        };
        let n = legends.len();
        let mut right = stage.w - 24.0;
        for (i, l) in legends.iter().enumerate() {
            let (key, label) = hint(l);
            let cache = format!("{key}:{label}");
            let (tex, w, h) = match self.text.get(&(cache.clone(), Style::Digit)) {
                Some(t) => *t,
                None => {
                    let f = match key {
                        "LR" | "UD" => arrows_hint_face(label),
                        _ => hint_face(key, label),
                    };
                    let t = (gfx.upload_rgba(f.w, f.h, &f.rgba), f.w, f.h);
                    self.text.insert((cache, Style::Digit), t);
                    t
                }
            };
            // Back on the left, the rest from the right, as slot lays them out.
            let x = if i == 0 && n > 0 {
                24.0
            } else {
                right -= w as f32;
                let x = right;
                right -= 16.0;
                x
            };
            out.push(Draw::Tex {
                x,
                y: FOOTER_Y,
                w: w as f32,
                h: h as f32,
                tex,
                alpha: 1.0,
            });
        }
    }
}

/// slot's `draw_studio_ground`: a green cutting mat with a 24 px grid.
fn ground(stage: Stage, out: &mut Vec<Draw>) {
    let (w, h) = (stage.w, OUT_H as f32);
    out.push(Draw::Rect {
        x: 0.0,
        y: 0.0,
        w,
        h,
        colour: MAT,
    });
    let line = |x: f32, y: f32, w: f32, h: f32, a: f32| Draw::Rect {
        x,
        y,
        w,
        h,
        colour: [0.82, 0.92, 0.78, a],
    };
    for i in 0..(w as usize / 24 + 1) {
        let a = if i % 5 == 0 { 0.34 } else { 0.13 };
        out.push(line(i as f32 * 24.0, 0.0, 1.0, h, a));
    }
    for i in 0..(h as usize / 24 + 1) {
        let a = if i % 5 == 0 { 0.34 } else { 0.13 };
        out.push(line(0.0, i as f32 * 24.0, w, 1.0, a));
    }
}

/// The cart face, back plastic and board first for a see-through cart,
/// scaled to the studio's cart and tilted by slot's `CART_TILT`.
fn tilted(cart: &Cart, stage: Stage, face: &slot_mini_ui::CartFace) -> Sprite {
    let mut flat = Sprite::new(face.w, face.h);
    if shows_interior(cart) {
        let gb = !matches!(cart.platform, Platform::Gba | Platform::Gbav);
        let shell = slot_mini_ui::gb_shell_of(cart);
        let whole = Placed {
            x: 0.0,
            y: 0.0,
            w: face.w as f32,
            h: face.h as f32,
        };
        let r = if gb { whole } else { board_in(whole) };
        for layer in [cart_interior(cart), cart_backing(shell)] {
            let s = Sprite::from_rgba(layer.w, layer.h, &layer.rgba)
                .resized(r.w.round() as u32, r.h.round() as u32);
            flat.draw_over(&s, r.x.round() as i32, r.y.round() as i32);
        }
    }
    flat.draw_over(&Sprite::from_rgba(face.w, face.h, &face.rgba), 0, 0);
    let l = layout(stage.w, cart.platform);
    flat.resized(l.cart_w.round() as u32, l.cart_h.round() as u32)
        .rotated(CART_TILT)
}

/// The face's label well (`[x, y, w, h]` in face pixels) as a sprite.
fn crop(face: &slot_mini_ui::CartFace, well: [u32; 4]) -> Sprite {
    let [x, y, w, h] = well;
    let (w, h) = (w.min(face.w - x.min(face.w)), h.min(face.h - y.min(face.h)));
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for row in y..y + h {
        let at = ((row * face.w + x) * 4) as usize;
        rgba.extend_from_slice(&face.rgba[at..at + (w * 4) as usize]);
    }
    Sprite::from_rgba(w, h, &rgba)
}
