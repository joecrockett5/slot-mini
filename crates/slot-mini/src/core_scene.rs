// Adapted from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot/src/app.rs (`open_core_picker`, `core_picker_input`,
//   `draw_core_picker`, `draw_board_chip`, `draw_lid_shadow`, the core
//   picker's constants) and crates/slot/src/face_builder.rs (the faces built
//   for the selected cart).
// Changes: GBA carts only (slot also offers Gambatte for Game Boy carts);
//   the choice is kept in Onion's per-game core file (cores.rs); the scene's
//   textures live here instead of on slot's App; the legends are slot-mini's
//   footer hints (B Cancel, left/right Swap, A Choose: slot's words),
//   spread as slot spreads them.
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

use std::sync::mpsc::{self, Receiver};

use slot_mini_gfx::{Button, Draw, Renderer, Stage, TexId};
use slot_mini_store::{Cart, Core, Platform};
use slot_mini_ui::{
    arrows_hint_face, board_face, board_from_shaped, cart_face_with, cart_interior, chip_face,
    chip_shadow_face, ease, grown, hint_face, lid_at, lid_from_shaped, lift_of, padded, seats,
    shelf_cart_shaped, socket_face, CartFace, Millis, Placed, Shelf, CART_W, FOOTER_Y, HOP_LIFT,
    SHADOW_H, SHADOW_W, TURN_PAD,
};

use crate::core_picker::{CorePicker, Outcome, Press};
use crate::paths::Paths;

/// slot: how far the other carts recede and dim while the cart is open.
pub const RECEDE: f32 = 0.26;
pub const DIM: f32 = 0.25 / (1.0 - RECEDE);
/// slot: `FACES_WAIT_MS`. The scene waits this long at most for the cart's
/// inside, board and lid to be built.
pub const FACES_WAIT_MS: Millis = 1500;
const LID_SHADOW_W: f32 = 168.0;
const LID_SHADOW_H: f32 = 18.0;
const LID_SHADOW_DROP: f32 = 27.1;
const LID_SHADOW_ALPHA: f32 = 0.8;

/// Whether a cart can choose its core here: GBA games, as the maintainer
/// asked (mGBA or gpSP).
pub fn offers(cart: &Cart) -> bool {
    cart.platform.hardware() == Platform::Gba
}

/// Textures every picker shares, made the first time one opens.
pub struct Statics {
    /// By `Core::index`.
    sockets: Vec<TexId>,
    chips: Vec<TexId>,
    blank: TexId,
    shadow: TexId,
    /// Dark and light.
    plugged: [TexId; 2],
    /// Cancel, Swap, Choose.
    legends: [(TexId, u32, u32); 3],
}

fn up(gfx: &mut Renderer, f: CartFace) -> TexId {
    gfx.upload_rgba(f.w, f.h, &f.rgba)
}

impl Statics {
    pub fn new(gfx: &mut Renderer) -> Statics {
        let sockets = Core::ALL.iter().map(|&c| up(gfx, socket_face(c))).collect();
        let chips = Core::ALL
            .iter()
            .map(|&c| up(gfx, chip_face(Some(c))))
            .collect();
        let blank = up(gfx, chip_face(None));
        let shadow = up(gfx, chip_shadow_face());
        let strip = |gfx: &mut Renderer, rgba: &[u8]| {
            gfx.upload_rgba(slot_mini_ui::SLOT_W, slot_mini_ui::SLOT_H, rgba)
        };
        let plugged = [
            strip(gfx, slot_mini_ui::slot_plugged()),
            strip(gfx, slot_mini_ui::slot_plugged_light()),
        ];
        let legend = |gfx: &mut Renderer, f: slot_mini_ui::UndoFace| {
            (gfx.upload_rgba(f.w, f.h, &f.rgba), f.w, f.h)
        };
        let legends = [
            legend(gfx, hint_face("B", "CANCEL")),
            legend(gfx, arrows_hint_face("SWAP")),
            legend(gfx, hint_face("A", "CHOOSE")),
        ];
        Statics {
            sockets,
            chips,
            blank,
            shadow,
            plugged,
            legends,
        }
    }
}

/// The selected cart's inside, board and lid (its face, padded for turning).
type Built = (CartFace, CartFace, CartFace);

struct CartTex {
    interior: TexId,
    board: TexId,
    lid: TexId,
}

/// The open cart: slot's core picker on the selected cart.
pub struct CoreScene {
    pub picker: CorePicker,
    pub stem: String,
    cart: Cart,
    building: Option<Receiver<Built>>,
    tex: Option<CartTex>,
}

impl CoreScene {
    /// slot's `open_core_picker`: the chip starts in the seat of the core
    /// the cart runs on now. The scene waits for its faces before opening.
    pub fn open(paths: &Paths, cart: &Cart, now: Millis) -> CoreScene {
        let seat = crate::cores::chosen(paths, cart);
        let picker = CorePicker::open(Core::pair(cart.platform), seat, now);
        let (tx, rx) = mpsc::channel();
        let (root, job) = (paths.sd.clone(), cart.clone());
        let spawned = std::thread::Builder::new()
            .name("slot-core-faces".into())
            .spawn(move || {
                let art = crate::label_cache::label_art(&root, &job);
                let lid = padded(&cart_face_with(&job, art), TURN_PAD);
                let _ = tx.send((cart_interior(&job), board_face(&job), lid));
            });
        if let Err(e) = spawned {
            eprintln!("slot-mini: core picker: faces thread failed to start: {e}");
        }
        CoreScene {
            picker,
            stem: cart.stem.clone(),
            cart: cart.clone(),
            building: Some(rx),
            tex: None,
        }
    }

    pub fn cart(&self) -> &Cart {
        &self.cart
    }

    /// Drawn once its faces are in, or it has waited long enough.
    pub fn shown(&self) -> bool {
        !self.picker.waiting()
    }

    fn ready(&self) -> bool {
        self.tex.is_some()
    }

    /// Collect the faces and start the opening when they are in.
    pub fn tick(&mut self, gfx: &mut Renderer, now: Millis) {
        if let Some(rx) = &self.building {
            if let Ok((interior, board, lid)) = rx.try_recv() {
                self.tex = Some(CartTex {
                    interior: up(gfx, interior),
                    board: up(gfx, board),
                    lid: up(gfx, lid),
                });
                self.building = None;
            }
        }
        if self.picker.waiting() && (self.ready() || self.picker.waited(now) >= FACES_WAIT_MS) {
            self.picker.start(now);
        }
    }

    pub fn finished(&self, now: Millis) -> bool {
        self.picker.finished(now)
    }

    /// slot's `core_picker_input`. `Some(core)` when A keeps a core.
    pub fn press(&mut self, b: Button, now: Millis) -> Option<Core> {
        let press = match b {
            Button::Left => Press::Left,
            Button::Right => Press::Right,
            Button::A => Press::Keep,
            Button::B | Button::Menu => Press::Back,
            _ => return None,
        };
        match self.picker.press(press, now) {
            Outcome::Write(core) => Some(core),
            Outcome::Nothing | Outcome::Refused => None,
        }
    }

    pub fn close(self, gfx: &mut Renderer) {
        if let Some(t) = self.tex {
            gfx.free(t.interior);
            gfx.free(t.board);
            gfx.free(t.lid);
        }
    }

    /// How open the cart is, eased, for receding and dimming the shelf.
    pub fn open_eased(&self, now: Millis) -> f32 {
        ease(self.picker.openness(now))
    }

    /// slot's `draw_plugged_slot` while the cart is open.
    pub fn draw_plugged(
        &self,
        stage: Stage,
        s: &Statics,
        light: bool,
        now: Millis,
        out: &mut Vec<Draw>,
    ) {
        let lift = lift_of(self.picker.openness(now));
        slot_mini_ui::draw_plugged_slot(stage, Some(s.plugged[usize::from(light)]), lift, out);
    }

    /// slot's `draw_core_picker`, `draw_board_chip` and `draw_lid_shadow`,
    /// then the legends.
    pub fn draw(&self, stage: Stage, shelf: &Shelf, s: &Statics, now: Millis, out: &mut Vec<Draw>) {
        let progress = self.picker.openness(now);
        let lift = lift_of(progress);
        let (rest, scale) = shelf.selected_at(stage);
        let shell = shelf.selected_shell();
        let at_shelf = shelf_cart_shaped(shell, rest, scale);
        let board = board_from_shaped(stage, shell, at_shelf, progress);

        if let Some(t) = &self.tex {
            for tex in [t.interior, t.board] {
                out.push(Draw::Tex {
                    x: board.x,
                    y: board.y,
                    w: board.w,
                    h: board.h,
                    tex,
                    alpha: 1.0,
                });
            }
            self.draw_board_chip(board, shell, s, lift, now, out);
        }

        // The lid's shadow on the board.
        let (lid, turn) = lid_from_shaped(stage, shell, at_shelf, progress);
        let k = lid.w / lid_at(stage, 1.0).0.w;
        let (w, h) = (LID_SHADOW_W * k, LID_SHADOW_H * k);
        out.push(Draw::Tex {
            x: lid.x + (lid.w - w) / 2.0,
            y: lid.y + lid.h + LID_SHADOW_DROP * k - h / 2.0,
            w,
            h,
            tex: s.shadow,
            alpha: LID_SHADOW_ALPHA * lift,
        });

        match &self.tex {
            Some(t) => {
                let at = grown(lid, TURN_PAD as f32 * lid.w / CART_W as f32);
                out.push(Draw::Turned {
                    x: at.x,
                    y: at.y,
                    w: at.w,
                    h: at.h,
                    tex: t.lid,
                    alpha: 1.0,
                    turn,
                });
            }
            None => {
                if let Some((_, Some(tex))) = shelf.find(&self.stem) {
                    out.push(Draw::Turned {
                        x: lid.x,
                        y: lid.y,
                        w: lid.w,
                        h: lid.h,
                        tex,
                        alpha: 1.0,
                        turn,
                    });
                }
            }
        }

        // slot's `draw_spread_legends`: Cancel, Swap and Choose spread
        // evenly between the footer's margins.
        let seen = |w: u32| w as f32;
        let room = stage.w - 2.0 * 24.0;
        let gap = (room - s.legends.iter().map(|l| seen(l.1)).sum::<f32>()) / 2.0;
        let mut x: f32 = 24.0;
        for &(tex, w, h) in &s.legends {
            out.push(Draw::Tex {
                x: x.round(),
                y: FOOTER_Y,
                w: w as f32,
                h: h as f32,
                tex,
                alpha: lift,
            });
            x += seen(w) + gap;
        }
    }

    fn draw_board_chip(
        &self,
        board: Placed,
        shell: Option<slot_mini_ui::GbShell>,
        s: &Statics,
        lift: f32,
        now: Millis,
        out: &mut Vec<Draw>,
    ) {
        let seats = seats(shell);
        let zoom = seats.zoom(board);
        for (place, core) in self.picker.pair().into_iter().enumerate() {
            let Some(&tex) = s.sockets.get(core.index()) else {
                continue;
            };
            let (x, y) = seats.at(board, seats.socket_u[place], seats.socket_v);
            out.push(Draw::Tex {
                x: x.round(),
                y: y.round(),
                w: seats.socket_size.0 * zoom,
                h: seats.socket_size.1 * zoom,
                tex,
                alpha: 1.0,
            });
        }
        let chip = self.picker.chip(now);
        let u = seats.chip_u[0] + (seats.chip_u[1] - seats.chip_u[0]) * chip.across;
        if chip.lift > 0.0 {
            let (du, dv) = seats.shadow_uv;
            let (cx, cy) = seats.at(board, u + du, seats.chip_v + dv);
            let (w, h) = (SHADOW_W as f32 * zoom, SHADOW_H as f32 * zoom);
            out.push(Draw::Tex {
                x: cx - w / 2.0,
                y: cy - h / 2.0,
                w,
                h,
                tex: s.shadow,
                alpha: 0.6 * chip.lift * lift,
            });
        }
        let face = match chip.seated {
            Some(core) => s.chips.get(core.index()).copied(),
            None => Some(s.blank),
        };
        if let Some(tex) = face {
            let (x, y) = seats.at(board, u, seats.chip_v - HOP_LIFT * chip.lift);
            let body = Placed {
                x: x + chip.shake,
                y,
                w: seats.chip_size.0 * zoom,
                h: seats.chip_size.1 * zoom,
            };
            let at = grown(body, TURN_PAD as f32 * zoom);
            out.push(Draw::Turned {
                x: at.x.round(),
                y: at.y.round(),
                w: at.w,
                h: at.h,
                tex,
                alpha: 1.0,
                turn: chip.tip,
            });
        }
    }
}
