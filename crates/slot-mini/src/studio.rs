// Copied from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot/src/studio.rs (the Studio state machine, up to `legends`)
// Changes: buttons are slot-mini-gfx's `Button` (aliased to slot's `Btn`);
//   the print and apply animation code after `legends` is not copied.
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

use slot_mini_gfx::Button as Btn;
use slot_mini_store::{Cart, Outline, Platform, ShellChoice, ShellFinish};
use slot_mini_ui::palette::{chip_grid, step, Chip, Dir, Placed};
use slot_mini_ui::{shell_for, shell_presets, Finish, Shell};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Label,
    LabelColor,
    Color,
    Finish,
    Shape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Shell,
    Label,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LabelKind {
    Existing,
    Real,
    Logo,
    Title,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    List {
        row: usize,
    },
    Sheet {
        at: usize,
    },
    Chips {
        target: Target,
        at: usize,
    },
    Custom {
        target: Target,
        rgb: [u8; 3],
        start: [u8; 3],
        channel: usize,
    },
    Hex {
        target: Target,
        digits: [u8; 6],
        pos: usize,
        start: [u8; 3],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Close,
    Fetch,
    Apply(LabelKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Legend {
    Back,
    Open,
    Change,
    ChangeVertical,
    Apply,
    Move,
    Reset,
    Hex,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Studio {
    pub cart: Cart,
    pub screen: Screen,
    pub wearing: LabelKind,
    pub shell: Option<ShellChoice>,
    pub ground: Option<[u8; 3]>,
    pub sheet: Vec<LabelKind>,
    wifi: bool,
    art_ready: bool,
    pub art_failed: bool,
    pub duplicates: bool,
    auto: ShellChoice,
    pub opened_ground: Option<[u8; 3]>,
    before_custom: (Option<ShellChoice>, Option<[u8; 3]>),
    logo_worn: bool,
    text_worn: bool,
    wheel: bool,
}

fn finish_of(shell: Shell) -> ShellFinish {
    match shell.finish {
        Finish::Solid => ShellFinish::Solid,
        Finish::Translucent => ShellFinish::Clear,
        Finish::Glitter => ShellFinish::Glitter,
    }
}

fn digits_of(rgb: [u8; 3]) -> [u8; 6] {
    [
        rgb[0] >> 4,
        rgb[0] & 15,
        rgb[1] >> 4,
        rgb[1] & 15,
        rgb[2] >> 4,
        rgb[2] & 15,
    ]
}

fn rgb_of(d: [u8; 6]) -> [u8; 3] {
    [d[0] * 16 + d[1], d[2] * 16 + d[3], d[4] * 16 + d[5]]
}

impl Studio {
    pub fn new(cart: Cart, wifi: bool) -> Studio {
        let mut plain = cart.clone();
        plain.shell = None;
        let auto = shell_for(&plain);
        let sheet = vec![LabelKind::Real, LabelKind::Logo];
        let artless = cart.label.is_none();
        let title = slot_mini_ui::label_text(&cart);
        let cart_shell = cart.shell;
        let mut studio = Studio {
            shell: cart_shell,
            cart,
            screen: Screen::List { row: 0 },
            wearing: LabelKind::Existing,
            ground: None,
            sheet,
            wifi,
            art_ready: false,
            art_failed: false,
            duplicates: false,
            auto: ShellChoice {
                outline: Outline::Auto,
                colour: auto.colour,
                finish: finish_of(auto),
            },
            opened_ground: None,
            before_custom: (cart_shell, None),
            logo_worn: false,
            text_worn: false,
            wheel: false,
        };
        if artless {
            studio.adopt_text(slot_mini_ui::label_colour(&title));
        }
        studio
    }

    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        if self.wifi {
            rows.push(Row::Label);
        }
        if self.wearing == LabelKind::Logo
            || self.wearing_text()
            || (self.wheel && self.wearing_old_logo())
        {
            rows.push(Row::LabelColor);
        }
        rows.extend([Row::Color, Row::Finish]);
        if matches!(self.cart.platform, Platform::Gb | Platform::Gbc) {
            rows.push(Row::Shape);
        }
        rows
    }

    fn wearing_old_logo(&self) -> bool {
        self.logo_worn && self.wearing == LabelKind::Existing
    }

    pub fn wearing_text(&self) -> bool {
        self.wearing == LabelKind::Title || (self.text_worn && self.wearing == LabelKind::Existing)
    }

    pub fn adopt_text(&mut self, ground: [u8; 3]) {
        self.text_worn = true;
        self.ground = Some(ground);
        self.opened_ground = Some(ground);
    }

    pub fn adopt_logo(&mut self, ground: [u8; 3]) {
        self.logo_worn = true;
        self.ground = Some(ground);
        self.opened_ground = Some(ground);
    }

    pub fn wheel_arrived(&mut self) {
        let was = self.row();
        self.wheel = true;
        if let (Some(was), Screen::List { .. }) = (was, self.screen) {
            let row = self.rows().iter().position(|r| *r == was).unwrap_or(0);
            self.screen = Screen::List { row };
        }
    }

    pub fn applied(&mut self, kind: LabelKind) {
        let other = self.sheet.iter().copied().find(|k| *k != kind);
        self.sheet = std::iter::once(self.wearing).chain(other).collect();
        if matches!(self.screen, Screen::Sheet { .. }) {
            self.screen = Screen::Sheet { at: 0 };
        }
        self.wearing = kind;
        let fresh = matches!(kind, LabelKind::Logo | LabelKind::Title);
        self.ground = if fresh { None } else { self.opened_ground };
        if fresh {
            self.opened_ground = None;
        }
    }

    fn tinted(&self) -> bool {
        self.wearing == LabelKind::Logo || self.wearing_old_logo() || self.wearing_text()
    }

    pub fn preview_ground(&self) -> Option<[u8; 3]> {
        self.ground.filter(|_| self.tinted())
    }

    pub fn recolored(&self) -> bool {
        self.tinted() && self.ground != self.opened_ground
    }

    pub fn shown_label(&self) -> LabelKind {
        if self.ground == self.opened_ground {
            self.wearing
        } else if self.wearing_old_logo() {
            LabelKind::Logo
        } else if self.wearing_text() {
            LabelKind::Title
        } else {
            self.wearing
        }
    }

    pub fn art_ready(&self) -> bool {
        self.art_ready
    }

    pub fn art_arrived(&mut self, real: bool, logo: bool) {
        self.art_ready = true;
        self.sheet.retain(|k| match k {
            LabelKind::Real => real,
            LabelKind::Logo => logo,
            _ => true,
        });
        if self.sheet.len() < 2 && self.cart.label.is_some() && !self.text_worn {
            self.sheet.push(LabelKind::Title);
        }
        if let Screen::Sheet { at } = self.screen {
            let at = at.min(self.sheet.len().saturating_sub(1));
            self.screen = Screen::Sheet { at };
        }
    }

    pub fn row(&self) -> Option<Row> {
        match self.screen {
            Screen::List { row } => self.rows().get(row).copied(),
            _ => None,
        }
    }

    pub fn current_shell(&self) -> ShellChoice {
        self.shell.unwrap_or(self.auto)
    }

    fn grid() -> Vec<Placed> {
        chip_grid(0.0, 0.0)
    }

    fn chip_in_use(&self, target: Target) -> usize {
        let (now, opened) = match target {
            Target::Shell => (
                self.shell.map(|c| c.colour),
                self.cart.shell.map(|c| c.colour),
            ),
            Target::Label => (self.ground, self.opened_ground),
        };
        let presets = shell_presets();
        let chip = match now {
            None if opened.is_some() => Some(Chip::Auto),
            None => None,
            Some(rgb) => presets
                .iter()
                .position(|(_, shell)| shell.colour == rgb)
                .map(Chip::Preset),
        };
        let grid = Self::grid();
        chip.and_then(|c| grid.iter().position(|p| p.chip == c))
            .unwrap_or(0)
    }

    pub fn chips_jump(&mut self, at: usize) {
        let Screen::Chips { target, .. } = self.screen else {
            return;
        };
        let grid = Self::grid();
        let at = at.min(grid.len() - 1);
        self.screen = Screen::Chips { target, at };
        let outline = self.current_shell().outline;
        let shaped = |base: Option<ShellChoice>, auto: ShellChoice| match base {
            Some(choice) => Some(ShellChoice { outline, ..choice }),
            None => (outline != Outline::Auto).then_some(ShellChoice { outline, ..auto }),
        };
        match (target, grid[at].chip) {
            (_, Chip::Custom) => {}
            (Target::Shell, Chip::Current) => self.shell = shaped(self.cart.shell, self.auto),
            (Target::Shell, Chip::Auto) => self.shell = shaped(None, self.auto),
            (Target::Shell, Chip::Preset(i)) => {
                let preset = shell_presets()[i].1;
                self.shell = Some(ShellChoice {
                    outline: self.current_shell().outline,
                    colour: preset.colour,
                    finish: finish_of(preset),
                });
            }
            (Target::Label, Chip::Current) => self.ground = self.opened_ground,
            (Target::Label, Chip::Auto) => self.ground = None,
            (Target::Label, Chip::Preset(i)) => self.ground = Some(shell_presets()[i].1.colour),
        }
    }

    fn set_color(&mut self, target: Target, rgb: [u8; 3]) {
        match target {
            Target::Shell => {
                self.shell = Some(ShellChoice {
                    colour: rgb,
                    ..self.current_shell()
                })
            }
            Target::Label => self.ground = Some(rgb),
        }
    }

    fn undo_custom(&mut self) {
        (self.shell, self.ground) = self.before_custom;
    }

    fn color_of(&self, target: Target) -> [u8; 3] {
        match target {
            Target::Shell => self.current_shell().colour,
            Target::Label => self.ground.unwrap_or([0x80, 0x80, 0x80]),
        }
    }

    fn step_finish(&mut self, forward: bool) {
        const ORDER: [ShellFinish; 3] =
            [ShellFinish::Solid, ShellFinish::Clear, ShellFinish::Glitter];
        let current = self.current_shell();
        let at = ORDER.iter().position(|f| *f == current.finish).unwrap_or(0);
        let next = if forward { at + 1 } else { at + 2 } % 3;
        self.shell = Some(ShellChoice {
            finish: ORDER[next],
            ..current
        });
    }

    fn step_shape(&mut self, forward: bool) {
        const ORDER: [Outline; 3] = [Outline::Auto, Outline::Notched, Outline::Rounded];
        let current = self.current_shell();
        let at = ORDER
            .iter()
            .position(|o| *o == current.outline)
            .unwrap_or(0);
        let next = if forward { at + 1 } else { at + 2 } % 3;
        self.shell = Some(ShellChoice {
            outline: ORDER[next],
            ..current
        });
    }

    pub fn nudge(&mut self, delta: i32) {
        if let Screen::Custom {
            target,
            mut rgb,
            start,
            channel,
        } = self.screen
        {
            rgb[channel] = (rgb[channel] as i32 + delta).clamp(0, 255) as u8;
            self.screen = Screen::Custom {
                target,
                rgb,
                start,
                channel,
            };
            self.set_color(target, rgb);
        }
    }

    fn press_list(&mut self, row: usize, btn: Btn) -> Option<Effect> {
        let rows = self.rows();
        let count = rows.len();
        match (btn, rows.get(row).copied()?) {
            (Btn::Up, _) => {
                self.screen = Screen::List {
                    row: (row + count - 1) % count,
                }
            }
            (Btn::Down, _) => {
                self.screen = Screen::List {
                    row: (row + 1) % count,
                }
            }
            (Btn::B, _) => return Some(Effect::Close),
            (Btn::A, Row::Label) => {
                self.screen = Screen::Sheet { at: 0 };
                return Some(Effect::Fetch);
            }
            (Btn::A, Row::Color) => {
                self.screen = Screen::Chips {
                    target: Target::Shell,
                    at: self.chip_in_use(Target::Shell),
                }
            }
            (Btn::A, Row::LabelColor) => {
                self.screen = Screen::Chips {
                    target: Target::Label,
                    at: self.chip_in_use(Target::Label),
                }
            }
            (Btn::Left | Btn::Right, Row::Finish) => self.step_finish(btn == Btn::Right),
            (Btn::Left | Btn::Right, Row::Shape) => self.step_shape(btn == Btn::Right),
            _ => {}
        }
        None
    }

    fn press_sheet(&mut self, at: usize, btn: Btn) -> Option<Effect> {
        let last = self.sheet.len().saturating_sub(1);
        match btn {
            Btn::Left | Btn::Up => {
                self.screen = Screen::Sheet {
                    at: at.saturating_sub(1),
                }
            }
            Btn::Right | Btn::Down => {
                self.screen = Screen::Sheet {
                    at: (at + 1).min(last),
                }
            }
            Btn::B => {
                let row = self
                    .rows()
                    .iter()
                    .position(|r| *r == Row::Label)
                    .unwrap_or(0);
                self.screen = Screen::List { row };
            }
            Btn::A => {
                let kind = *self.sheet.get(at)?;
                let fetched = matches!(kind, LabelKind::Real | LabelKind::Logo);
                if kind == self.wearing || (fetched && !self.art_ready) {
                    return None;
                }
                return Some(Effect::Apply(kind));
            }
            _ => {}
        }
        None
    }

    fn back_to_list(&mut self, want: Row) {
        let row = self.rows().iter().position(|r| *r == want).unwrap_or(0);
        self.screen = Screen::List { row };
    }

    fn press_chips(&mut self, target: Target, at: usize, btn: Btn) {
        let grid = Self::grid();
        let dir = match btn {
            Btn::Left => Dir::Left,
            Btn::Right => Dir::Right,
            Btn::Up => Dir::Up,
            Btn::Down => Dir::Down,
            Btn::B => {
                let row = match target {
                    Target::Shell => Row::Color,
                    Target::Label => Row::LabelColor,
                };
                return self.back_to_list(row);
            }
            Btn::A if grid[at].chip == Chip::Custom => {
                let rgb = self.color_of(target);
                self.before_custom = (self.shell, self.ground);
                self.screen = Screen::Custom {
                    target,
                    rgb,
                    start: rgb,
                    channel: 0,
                };
                return;
            }
            _ => return,
        };
        self.chips_jump(step(&grid, at, dir));
    }

    pub fn press(&mut self, btn: Btn) -> Option<Effect> {
        match self.screen {
            Screen::List { row } => return self.press_list(row, btn),
            Screen::Sheet { at } => return self.press_sheet(at, btn),
            Screen::Chips { target, at } => self.press_chips(target, at, btn),
            Screen::Custom {
                target,
                rgb,
                start,
                channel,
            } => match btn {
                Btn::Up => {
                    self.screen = Screen::Custom {
                        target,
                        rgb,
                        start,
                        channel: (channel + 2) % 3,
                    }
                }
                Btn::Down => {
                    self.screen = Screen::Custom {
                        target,
                        rgb,
                        start,
                        channel: (channel + 1) % 3,
                    }
                }
                Btn::Left => self.nudge(-1),
                Btn::Right => self.nudge(1),
                Btn::X => {
                    self.screen = Screen::Custom {
                        target,
                        rgb: start,
                        start,
                        channel,
                    };
                    self.undo_custom();
                }
                Btn::Y => {
                    self.screen = Screen::Hex {
                        target,
                        digits: digits_of(rgb),
                        pos: 0,
                        start,
                    }
                }
                Btn::B => {
                    let at = Self::grid()
                        .iter()
                        .position(|p| p.chip == Chip::Custom)
                        .unwrap_or(0);
                    self.screen = Screen::Chips { target, at };
                }
                _ => {}
            },
            Screen::Hex {
                target,
                mut digits,
                pos,
                start,
            } => match btn {
                Btn::Left | Btn::Right => {
                    let pos = if btn == Btn::Left {
                        (pos + 5) % 6
                    } else {
                        (pos + 1) % 6
                    };
                    self.screen = Screen::Hex {
                        target,
                        digits,
                        pos,
                        start,
                    };
                }
                Btn::Up | Btn::Down => {
                    let by = if btn == Btn::Up { 1 } else { 15 };
                    digits[pos] = (digits[pos] + by) % 16;
                    self.screen = Screen::Hex {
                        target,
                        digits,
                        pos,
                        start,
                    };
                    self.set_color(target, rgb_of(digits));
                }
                Btn::X => {
                    self.screen = Screen::Hex {
                        target,
                        digits: digits_of(start),
                        pos,
                        start,
                    };
                    self.undo_custom();
                }
                Btn::B => {
                    self.screen = Screen::Custom {
                        target,
                        rgb: rgb_of(digits),
                        start,
                        channel: 0,
                    };
                }
                _ => {}
            },
        }
        None
    }

    pub fn legends(&self) -> Vec<Legend> {
        match self.screen {
            Screen::List { .. } => match self.row() {
                Some(Row::Finish | Row::Shape) => vec![Legend::Back, Legend::Change],
                _ => vec![Legend::Back, Legend::Open],
            },
            Screen::Sheet { .. } => vec![Legend::Back, Legend::Apply],
            Screen::Chips { at, .. } => match Self::grid().get(at).map(|p| p.chip) {
                Some(Chip::Custom) => vec![Legend::Back, Legend::Open],
                _ => vec![Legend::Back],
            },
            Screen::Custom { .. } => {
                vec![Legend::Back, Legend::Change, Legend::Reset, Legend::Hex]
            }
            Screen::Hex { .. } => vec![
                Legend::Back,
                Legend::Move,
                Legend::ChangeVertical,
                Legend::Reset,
            ],
        }
    }
}
