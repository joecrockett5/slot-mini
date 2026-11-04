// Adapted from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot/src/cart_faces.rs
// Changes: workers also convert faces to sprites and pre-scale them to
//   slot's side scale; the glue to slot's App and GL compositor is replaced by `sync`,
//   which hands finished faces back to the caller and can wait with a
//   deadline; `Building`, the worker loop and the tests are slot's.
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use std::time::{Duration, Instant};

use slot_mini_gfx::Sprite;
use slot_mini_store::Cart;
use slot_mini_ui::{cart_face_with, cart_interior, shows_interior, CartFace, SIDE_SCALE};

use crate::label_cache::label_art;

const WORKERS: usize = 2;

type Key = (usize, usize);

/// A cart's face and, for a clear GBA cart, the game-coloured inside it is
/// drawn over; each as a drawable sprite plus a copy at slot's side scale.
type Built = (Key, Prepared, Option<Prepared>);

/// A face ready for the renderer: full size and pre-scaled for the sides.
pub struct Prepared {
    pub full: Sprite,
    pub side: Sprite,
}

fn prepare(face: &CartFace) -> Prepared {
    let full = Sprite::from_rgba(face.w, face.h, &face.rgba);
    let side = full.resized(
        (face.w as f32 * SIDE_SCALE).round() as u32,
        (face.h as f32 * SIDE_SCALE).round() as u32,
    );
    Prepared { full, side }
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<(Key, Cart)>,
    closed: bool,
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

#[derive(Default)]
struct Building {
    carts: HashMap<Key, Cart>,
    outdated: HashSet<Key>,
}

impl Building {
    fn start(&mut self, key: Key, cart: &Cart) -> bool {
        match self.carts.get(&key) {
            None => {
                self.carts.insert(key, cart.clone());
                true
            }
            Some(building) => {
                if building != cart {
                    self.outdated.insert(key);
                }
                false
            }
        }
    }

    fn unqueue(&mut self, key: Key) {
        self.carts.remove(&key);
        self.outdated.remove(&key);
    }

    fn finish(&mut self, key: Key) -> bool {
        self.carts.remove(&key);
        !self.outdated.remove(&key)
    }
}

/// A face the app wants built: which shelf and cart, and whether it is on
/// screen now (worth waiting for) or only nearby.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FaceWant {
    pub shelf: usize,
    pub index: usize,
    pub on_screen: bool,
}

pub struct CartFaces {
    queue: Shared,
    built: Receiver<Built>,
    building: Building,
    asked: Vec<FaceWant>,
}

/// A finished face, and the inside drawn under a see-through cart.
pub struct Ready {
    pub shelf: usize,
    pub index: usize,
    pub face: Prepared,
    pub interior: Option<Prepared>,
}

impl CartFaces {
    pub fn spawn(root: PathBuf) -> Self {
        let queue: Shared = Arc::default();
        let (outbox, built) = mpsc::channel();
        for n in 0..WORKERS {
            let (queue, outbox, root) = (queue.clone(), outbox.clone(), root.clone());
            let spawned = thread::Builder::new()
                .name(format!("slot-carts-{n}"))
                .spawn(move || work(&queue, &outbox, &root));
            if let Err(e) = spawned {
                eprintln!("slot-mini: cart faces: worker thread failed to start: {e}");
            }
        }
        CartFaces {
            queue,
            built,
            building: Building::default(),
            asked: Vec::new(),
        }
    }

    /// Queue what is wanted, collect what is done, and wait up to `wait` for
    /// faces that are on screen (slot waits for them too, so carts never
    /// flash in as plain colour). `None` waits as long as it takes.
    pub fn sync<'a>(
        &mut self,
        wants: Vec<FaceWant>,
        cart: impl Fn(usize, usize) -> Option<&'a Cart>,
        wait: Option<Duration>,
    ) -> Vec<Ready> {
        let mut out = Vec::new();
        while let Ok(built) = self.built.try_recv() {
            self.land(built, &mut out);
        }
        if wants != self.asked {
            self.requeue(&wants, &cart);
            self.asked = wants;
        }
        let mut waiting: Vec<Key> = self
            .asked
            .iter()
            .filter(|w| w.on_screen)
            .map(|w| (w.shelf, w.index))
            .filter(|k| self.building.carts.contains_key(k))
            .collect();
        let deadline = wait.map(|w| Instant::now() + w);
        while !waiting.is_empty() {
            let got = match deadline {
                None => self.built.recv().ok(),
                Some(d) => {
                    let left = d.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    self.built.recv_timeout(left).ok()
                }
            };
            let Some(built) = got else {
                break;
            };
            waiting.retain(|k| *k != built.0);
            self.land(built, &mut out);
        }
        out
    }

    fn land(&mut self, (key, face, interior): Built, out: &mut Vec<Ready>) {
        self.asked.retain(|w| (w.shelf, w.index) != key);
        if !self.building.finish(key) {
            return;
        }
        out.push(Ready {
            shelf: key.0,
            index: key.1,
            face,
            interior,
        });
    }

    fn requeue<'a>(
        &mut self,
        wants: &[FaceWant],
        cart: &impl Fn(usize, usize) -> Option<&'a Cart>,
    ) {
        let (lock, wake) = &*self.queue;
        let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
        for (key, _) in queue.jobs.drain(..) {
            self.building.unqueue(key);
        }
        for w in wants {
            let key = (w.shelf, w.index);
            if let Some(cart) = cart(w.shelf, w.index) {
                if self.building.start(key, cart) {
                    queue.jobs.push_back((key, cart.clone()));
                }
            }
        }
        wake.notify_all();
    }

    /// Forget everything asked so far (the shelves were restocked).
    pub fn reset(&mut self) {
        let (lock, _) = &*self.queue;
        let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
        queue.jobs.clear();
        for k in self.building.carts.keys().copied().collect::<Vec<_>>() {
            self.building.outdated.insert(k);
        }
        self.asked.clear();
    }
}

impl Drop for CartFaces {
    fn drop(&mut self) {
        let (lock, wake) = &*self.queue;
        lock.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        wake.notify_all();
    }
}

fn work(queue: &Shared, outbox: &Sender<Built>, root: &Path) {
    let (lock, wake) = &**queue;
    loop {
        let (key, cart) = {
            let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if q.closed {
                    return;
                }
                if let Some(job) = q.jobs.pop_front() {
                    break job;
                }
                q = wake.wait(q).unwrap_or_else(|e| e.into_inner());
            }
        };
        slot_mini_store::header_index::verify(&cart.rom);
        let face = prepare(&cart_face_with(&cart, label_art(root, &cart)));
        let interior = shows_interior(&cart).then(|| prepare(&cart_interior(&cart)));
        if outbox.send((key, face, interior)).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slot_mini_store::Platform;

    fn cart(stem: &str) -> Cart {
        Cart {
            platform: Platform::Gba,
            stem: stem.into(),
            rom: "a.gba".into(),
            label: None,
            title: stem.into(),
            code: String::new(),
            shell: None,
        }
    }

    #[test]
    fn a_face_built_from_an_old_cart_is_thrown_away() {
        let mut b = Building::default();
        let old = cart("A");
        let mut new = old.clone();
        new.label = Some("new.png".into());
        assert!(b.start((0, 0), &old));
        assert!(!b.start((0, 0), &new));
        assert!(!b.finish((0, 0)));
        assert!(b.start((0, 0), &new));
        assert!(b.finish((0, 0)));
    }

    #[test]
    fn the_same_cart_asked_twice_builds_once_and_lands() {
        let mut b = Building::default();
        assert!(b.start((0, 0), &cart("A")));
        assert!(!b.start((0, 0), &cart("A")));
        assert!(b.finish((0, 0)));
    }
}
