//! The on-device cart studio (SELECT): slot's `Studio` state machine and
//! `studio_view` layout, drawn flat. The cart is tilted once in software
//! instead of by slot's GL quads, and printed labels appear as plain slips
//! without the printer animation (see docs/DESIGN-PARITY.md).

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
use crate::studio::{Effect, LabelKind, Legend, Screen, Studio};
use crate::studio_view::{layout, tray, view, Align, Item, Style, CART_TILT, SLIP_PAD, TRAY_TOP};

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
}

const MAT: [f32; 4] = [0.122, 0.267, 0.216, 1.0];
const PAPER: [f32; 4] = [0.957, 0.945, 0.918, 1.0];

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
        }
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
        match self.studio.press(b) {
            Some(Effect::Close) => return true,
            Some(Effect::Fetch) => self.fetch(paths, service, wifi),
            Some(Effect::Apply(kind)) => self.apply(kind, paths, gfx),
            None => {}
        }
        false
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

    fn apply(&mut self, kind: LabelKind, paths: &Paths, gfx: &mut Renderer) {
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
            LabelKind::Existing => None,
        };
        let Some(png) = png else {
            return;
        };
        if let Err(e) = make::relabel(&paths.sd, &cart, &png) {
            eprintln!("slot-mini: studio: label: {e}");
            return;
        }
        self.studio.cart.label = Some(make::label_path(&paths.sd, &cart));
        self.studio.applied(kind);
        self.relabelled = true;
        self.invalidate(gfx);
    }

    fn invalidate(&mut self, gfx: &mut Renderer) {
        if let Some((t, ..)) = self.cart_tex.take() {
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
        if let Some(t) = self.chip_disc.take() {
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
        let mut flat = Sprite::new(face.w, face.h);
        if shows_interior(&cart) {
            // A see-through cart: back plastic, then the board, then the shell.
            let gb = !matches!(cart.platform, Platform::Gba | Platform::Gbav);
            let shell = slot_mini_ui::gb_shell_of(&cart);
            let r = match gb {
                true => Placed {
                    x: 0.0,
                    y: 0.0,
                    w: face.w as f32,
                    h: face.h as f32,
                },
                false => board_in(Placed {
                    x: 0.0,
                    y: 0.0,
                    w: face.w as f32,
                    h: face.h as f32,
                }),
            };
            for layer in [cart_interior(&cart), cart_backing(shell)] {
                let s = Sprite::from_rgba(layer.w, layer.h, &layer.rgba)
                    .resized(r.w.round() as u32, r.h.round() as u32);
                flat.draw_over(&s, r.x.round() as i32, r.y.round() as i32);
            }
        }
        flat.draw_over(&Sprite::from_rgba(face.w, face.h, &face.rgba), 0, 0);
        let l = layout(stage.w, cart.platform);
        let scaled = flat.resized(l.cart_w.round() as u32, l.cart_h.round() as u32);
        let turned = scaled.rotated(CART_TILT);
        let (w, h) = (turned.w, turned.h);
        self.cart_tex = Some((gfx.upload(turned), w, h));
        self.built_for = Some(key);
        self.cart_tex
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
        ground(stage, out);
        let l = layout(stage.w, self.studio.cart.platform);
        let shown = view(&self.studio, stage.w);
        for item in shown.items {
            match item {
                Item::Cart => {
                    if let Some((tex, w, h)) = self.cart_face(paths, stage, gfx) {
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
                Item::Sheet if matches!(self.printer, Printer::Ready) => self.sheet(stage, out),
                Item::Sheet => {}
            }
        }
        if let Screen::Sheet { .. } = self.studio.screen {
            self.printer_message(stage, &l, gfx, out);
        }
        slot_mini_ui::draw_empty_slot(stage, None, out);
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

    fn sheet(&mut self, stage: Stage, out: &mut Vec<Draw>) {
        let t = tray(stage.w, &self.studio.cart);
        let at = match self.studio.screen {
            Screen::Sheet { at } => at,
            _ => 0,
        };
        for (i, kind) in self.studio.sheet.clone().into_iter().enumerate() {
            let y = TRAY_TOP + i as f32 * t.bottom;
            let lift = if i == at { 4.0 } else { 0.0 };
            if i == at {
                out.push(Draw::Rect {
                    x: t.x - 3.0,
                    y: y - 3.0 - lift,
                    w: t.slip_w + 6.0,
                    h: t.slip_h + 6.0,
                    colour: [1.0, 1.0, 1.0, 1.0],
                });
            }
            out.push(Draw::Rect {
                x: t.x,
                y: y - lift,
                w: t.slip_w,
                h: t.slip_h,
                colour: PAPER,
            });
            if let Some((_, tex, ..)) = self.prints.get(&kind) {
                out.push(Draw::Tex {
                    x: t.x + SLIP_PAD,
                    y: y - lift + SLIP_PAD,
                    w: t.sticker_w,
                    h: t.sticker_h,
                    tex: *tex,
                    alpha: if i == at { 1.0 } else { 0.6 },
                });
            }
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
        if matches!(self.studio.screen, Screen::Sheet { .. })
            && !matches!(self.printer, Printer::Ready)
        {
            legends.retain(|l| *l != Legend::Apply);
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
