//! The planner: a pure function from two manifests and the base to what
//! must move (docs/SYNC.md, "The planner" and "Text merges").

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{json, Value};

use crate::key::{GameId, Key, Kind};
use crate::layout::Layout;
use crate::model::{Item, Manifest};
use crate::names::fold;
use crate::report::{strings, Scope, Side};

/// Space the planner leaves free on the client.
pub const RESERVE: u64 = 16 * 1024 * 1024;

/// What was last synced with a device: folded key to hash (file items) or
/// value (text items).
pub type Base = HashMap<String, String>;

pub struct Input<'a> {
    pub scope: &'a Scope,
    pub client: &'a Manifest,
    pub hub: &'a Manifest,
    pub base: &'a Base,
    /// Games removed everywhere that this device has not removed yet.
    pub tombstones: &'a [GameId],
    /// The game running on the PC.
    pub busy: Option<&'a GameId>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// Remove the game from the client.
    Remove { game: GameId },
    /// Hub to client.
    Pull {
        key: Key,
        size: u64,
        hash: String,
        new: bool,
    },
    /// Client to hub.
    Push {
        key: Key,
        size: u64,
        hash: String,
        new: bool,
    },
    /// A save changed on both: `size` and `hash` are the hub's, `mine` the
    /// client's hash.
    Conflict {
        key: Key,
        winner: Side,
        size: u64,
        hash: String,
        mine: String,
    },
}

impl Op {
    pub fn key(&self) -> Option<&Key> {
        match self {
            Op::Remove { .. } => None,
            Op::Pull { key, .. } | Op::Push { key, .. } | Op::Conflict { key, .. } => Some(key),
        }
    }

    pub fn to_json(&self, id: usize) -> Value {
        match self {
            Op::Remove { game } => json!({"id": id, "op": "remove", "game": game.to_string()}),
            Op::Pull {
                key,
                size,
                hash,
                new,
            } => json!({"id": id, "op": "pull", "key": key.to_string(),
                        "size": size, "hash": hash, "new": new}),
            Op::Push {
                key,
                size,
                hash,
                new,
            } => json!({"id": id, "op": "push", "key": key.to_string(),
                        "size": size, "hash": hash, "new": new}),
            Op::Conflict {
                key,
                winner,
                size,
                hash,
                mine,
            } => json!({"id": id, "op": "conflict", "key": key.to_string(),
                        "winner": winner.as_str(), "size": size, "hash": hash, "mine": mine}),
        }
    }

    pub fn from_json(v: &Value) -> Option<Op> {
        let key = || Key::parse(v["key"].as_str()?);
        let hash = |name: &str| v[name].as_str().map(str::to_string);
        Some(match v["op"].as_str()? {
            "remove" => Op::Remove {
                game: GameId::parse(v["game"].as_str()?)?,
            },
            "pull" => Op::Pull {
                key: key()?,
                size: v["size"].as_u64()?,
                hash: hash("hash")?,
                new: v["new"].as_bool()?,
            },
            "push" => Op::Push {
                key: key()?,
                size: v["size"].as_u64()?,
                hash: hash("hash")?,
                new: v["new"].as_bool()?,
            },
            "conflict" => Op::Conflict {
                key: key()?,
                winner: Side::parse(v["winner"].as_str()?),
                size: v["size"].as_u64()?,
                hash: hash("hash")?,
                mine: hash("mine")?,
            },
            _ => return None,
        })
    }
}

/// Edits to one side's shell lines and favourites.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEdits {
    pub shell_set: Vec<(String, String)>,
    pub shell_remove: Vec<String>,
    pub fav_add: Vec<GameId>,
    pub fav_remove: Vec<GameId>,
}

impl TextEdits {
    pub fn is_empty(&self) -> bool {
        self.shell_set.is_empty()
            && self.shell_remove.is_empty()
            && self.fav_add.is_empty()
            && self.fav_remove.is_empty()
    }

    pub fn to_json(&self) -> Value {
        let lines = |games: &[GameId]| -> Vec<String> {
            games.iter().map(|g| g.favourite_line()).collect()
        };
        json!({
            "shells": {
                "set": self.shell_set.iter().map(|(s, v)| json!([s, v])).collect::<Vec<_>>(),
                "remove": self.shell_remove,
            },
            "favourites": {"add": lines(&self.fav_add), "remove": lines(&self.fav_remove)},
        })
    }

    pub fn from_json(v: &Value) -> TextEdits {
        let games = |v: &Value| -> Vec<GameId> {
            strings(v)
                .iter()
                .filter_map(|line| slot_mini_store::list_entry(line))
                .map(|(sys, stem)| GameId::new(sys, stem))
                .collect()
        };
        TextEdits {
            shell_set: v["shells"]["set"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|e| Some((e[0].as_str()?.into(), e[1].as_str()?.into())))
                        .collect()
                })
                .unwrap_or_default(),
            shell_remove: strings(&v["shells"]["remove"]),
            fav_add: games(&v["favourites"]["add"]),
            fav_remove: games(&v["favourites"]["remove"]),
        }
    }
}

/// What a plan leaves out, and why.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notes {
    pub didnt_fit: Vec<(Key, u64)>,
    pub busy: Vec<GameId>,
    pub unstorable: Vec<(Key, Side)>,
    pub duplicates: Vec<(Key, Side)>,
}

impl Notes {
    pub fn to_json(&self) -> Value {
        let sided = |list: &[(Key, Side)]| -> Vec<Value> {
            list.iter()
                .map(|(k, s)| json!({"key": k.to_string(), "side": s.as_str()}))
                .collect()
        };
        json!({
            "didnt_fit": self.didnt_fit.iter()
                .map(|(k, n)| json!({"key": k.to_string(), "size": n})).collect::<Vec<_>>(),
            "busy": self.busy.iter().map(|g| g.to_string()).collect::<Vec<_>>(),
            "unstorable": sided(&self.unstorable),
            "duplicates": sided(&self.duplicates),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// In the order the client carries them out; an action's id is its
    /// place here.
    pub actions: Vec<Op>,
    /// Base entries to write at once: items already the same on both
    /// sides (a hash), or gone from both (`None`).
    pub records: Vec<(String, Option<String>)>,
    /// Text edits for the hub, which it applies when it plans.
    pub hub_text: TextEdits,
    /// Text edits for the client.
    pub client_text: TextEdits,
    /// Base entries for text items, written when the client commits them.
    pub text_records: Vec<(String, Option<String>)>,
    pub notes: Notes,
}

impl Plan {
    /// The reply to `/sync/plan`.
    pub fn to_json(&self) -> Value {
        json!({
            "actions": self.actions.iter().enumerate()
                .map(|(id, op)| op.to_json(id)).collect::<Vec<_>>(),
            "text": self.client_text.to_json(),
            "notes": self.notes.to_json(),
        })
    }
}

/// What one row of the truth table says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Row {
    Nothing,
    Record,
    Forget,
    Push,
    Pull,
    Conflict,
}

/// The truth table: base, client and hub, each a hash or absent.
pub(crate) fn row(b: Option<&str>, c: Option<&str>, h: Option<&str>) -> Row {
    match (b, c, h) {
        (None, None, None) => Row::Nothing,
        (Some(_), None, None) => Row::Forget,
        (b, Some(x), Some(y)) if x == y => {
            if b == Some(x) {
                Row::Nothing
            } else {
                Row::Record
            }
        }
        (None, Some(_), None) => Row::Push,
        (None, None, Some(_)) => Row::Pull,
        (None, Some(_), Some(_)) => Row::Conflict,
        (Some(b), Some(x), Some(y)) => {
            if x == b {
                Row::Pull
            } else if y == b {
                Row::Push
            } else {
                Row::Conflict
            }
        }
        // Deletions do not propagate: the file comes back.
        (Some(_), None, Some(_)) => Row::Pull,
        (Some(_), Some(_), None) => Row::Push,
    }
}

/// A three-way merge of one text entry, where removals count as edits and
/// the hub wins a conflict.
pub(crate) fn merge<'a>(
    b: Option<&'a str>,
    c: Option<&'a str>,
    h: Option<&'a str>,
) -> Option<&'a str> {
    if c == h || b == c {
        h
    } else if b == h {
        c
    } else {
        h
    }
}

/// How long ago a save was played, by the clock of the side that has it.
fn age(now: u64, played: Option<u64>) -> Option<u64> {
    played.map(|p| now.saturating_sub(p))
}

struct Candidate<'a> {
    folded: String,
    c: Option<&'a Item>,
    h: Option<&'a Item>,
    row: Row,
}

pub fn plan(input: &Input) -> Plan {
    let (scope, client, hub, base) = (input.scope, input.client, input.hub, input.base);
    let mut out = Plan::default();
    let whole = scope.whole_library();
    let mut tombstones: Vec<&GameId> = if whole {
        input.tombstones.iter().collect()
    } else {
        Vec::new()
    };
    tombstones.sort_by_key(|g| g.folded());
    tombstones.dedup_by_key(|g| g.folded());
    let pending: HashSet<String> = tombstones.iter().map(|g| g.folded()).collect();
    let busy = input.busy.map(|g| g.folded());

    let index = |m: &Manifest| -> HashMap<String, usize> {
        m.items
            .iter()
            .enumerate()
            .map(|(n, i)| (i.key.folded(), n))
            .collect()
    };
    let (c_index, h_index) = (index(client), index(hub));
    let c_item = |k: &str| c_index.get(k).map(|&n| &client.items[n]);
    let h_item = |k: &str| h_index.get(k).map(|&n| &hub.items[n]);

    // Every key either side or the base knows, in key order.
    let mut keys: BTreeMap<String, Key> = BTreeMap::new();
    for folded in base.keys() {
        if let Some(key) = Key::parse(folded) {
            keys.insert(folded.clone(), key);
        }
    }
    for item in hub.items.iter().chain(&client.items) {
        keys.insert(item.key.folded(), item.key.clone());
    }

    let mut small = Vec::new();
    let mut saves = Vec::new();
    let mut roms = Vec::new();
    for (folded, key) in keys {
        if !scope.covers(&key) {
            continue;
        }
        let game = key.game().folded();
        // A removed game's items go from the client first; what the hub
        // has of it was put there after the removal, and is new.
        let gone = pending.contains(&game);
        if busy.as_ref() == Some(&game) {
            if let (Some(g), true) = (input.busy, out.notes.busy.is_empty()) {
                out.notes.busy.push(g.clone());
            }
            continue;
        }
        let (c, h) = (c_item(&folded).filter(|_| !gone), h_item(&folded));
        let row = row(
            base.get(&folded).map(String::as_str).filter(|_| !gone),
            c.map(|i| i.hash.as_str()),
            h.map(|i| i.hash.as_str()),
        );
        match row {
            Row::Nothing => continue,
            Row::Record => {
                out.records.push((folded, c.map(|i| i.hash.clone())));
                continue;
            }
            Row::Forget => {
                out.records.push((folded, None));
                continue;
            }
            _ => {}
        }
        let cand = Candidate { folded, c, h, row };
        match key.kind {
            Kind::Rom => roms.push(cand),
            Kind::Save => saves.push(cand),
            Kind::Label | Kind::Art => small.push(cand),
        }
    }

    // Which copy of a conflicting save was played last. A game's `.rtc`
    // goes the way of its `.srm`.
    let winner = |key: &Key, c: &Item, h: &Item| -> Side {
        let srm = Key::new(Kind::Save, key.sys, format!("{}.srm", key.stem())).folded();
        let c_age = age(client.now, c_item(&srm).unwrap_or(c).played);
        let h_age = age(hub.now, h_item(&srm).unwrap_or(h).played);
        match (c_age, h_age) {
            (Some(c), Some(h)) if c < h => Side::Client,
            _ => Side::Hub,
        }
    };
    let mut notes = std::mem::take(&mut out.notes);
    let mut resolve = |cand: &Candidate| -> Option<Op> {
        match (cand.row, cand.c, cand.h) {
            (Row::Push, Some(c), h) => {
                if h.is_none() && !hub.names.can_store(&Layout::store_name(&c.key)) {
                    notes.unstorable.push((c.key.clone(), Side::Hub));
                    return None;
                }
                Some(Op::Push {
                    key: c.key.clone(),
                    size: c.size,
                    hash: c.hash.clone(),
                    new: h.is_none(),
                })
            }
            (Row::Pull, c, Some(h)) => {
                if c.is_none() && !client.names.can_store(&Layout::store_name(&h.key)) {
                    notes.unstorable.push((h.key.clone(), Side::Client));
                    return None;
                }
                Some(Op::Pull {
                    key: c.map_or(&h.key, |c| &c.key).clone(),
                    size: h.size,
                    hash: h.hash.clone(),
                    new: c.is_none(),
                })
            }
            (Row::Conflict, Some(c), Some(h)) if c.key.kind == Kind::Save => Some(Op::Conflict {
                key: c.key.clone(),
                winner: winner(&c.key, c, h),
                size: h.size,
                hash: h.hash.clone(),
                mine: c.hash.clone(),
            }),
            // Labels, art and ROMs: the hub's copy wins.
            (Row::Conflict, Some(c), Some(h)) => Some(Op::Pull {
                key: c.key.clone(),
                size: h.size,
                hash: h.hash.clone(),
                new: false,
            }),
            _ => None,
        }
    };
    let mut first: Vec<(Kind, String, Op)> = small
        .iter()
        .chain(&saves)
        .filter_map(|cand| {
            let op = resolve(cand)?;
            Some((op.key()?.kind, cand.folded.clone(), op))
        })
        .collect();
    let rom_ops: Vec<Op> = roms.iter().filter_map(&mut resolve).collect();
    out.notes = notes;

    // Space: what the small files need comes off first, then ROMs in key
    // order; one that does not fit is left out and the rest are tried.
    let pulled = |op: &Op| match op {
        Op::Pull { size, .. } => *size,
        Op::Conflict {
            winner: Side::Hub,
            size,
            ..
        } => *size,
        _ => 0,
    };
    let mut budget = client.free.map(|free| {
        free.saturating_sub(RESERVE)
            .saturating_sub(first.iter().map(|(_, _, op)| pulled(op)).sum())
    });
    let mut last = Vec::new();
    for op in rom_ops {
        if let (Some(left), Op::Pull { key, size, .. }) = (budget.as_mut(), &op) {
            if *size > *left {
                out.notes.didnt_fit.push((key.clone(), *size));
                continue;
            }
            *left -= *size;
        }
        last.push(op);
    }

    // Saves need their game on both sides, there already or arriving.
    let games = |m: &Manifest, arriving: &dyn Fn(&Op) -> bool| -> HashSet<String> {
        m.games
            .iter()
            .map(|g| g.folded())
            .chain(
                last.iter()
                    .filter(|op| arriving(op))
                    .filter_map(|op| Some(op.key()?.game().folded())),
            )
            .collect()
    };
    let c_games = games(client, &|op| matches!(op, Op::Pull { .. }));
    let h_games = games(hub, &|op| matches!(op, Op::Push { .. }));
    first.retain(|(kind, _, op)| {
        *kind != Kind::Save
            || op.key().is_some_and(|k| {
                let game = k.game().folded();
                c_games.contains(&game) && h_games.contains(&game)
            })
    });
    first.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));

    out.actions = tombstones
        .iter()
        .map(|g| Op::Remove { game: (*g).clone() })
        .chain(first.into_iter().map(|(_, _, op)| op))
        .chain(last)
        .collect();

    if whole {
        merge_text(input, &pending, &mut out);
    }
    out
}

/// Merge shell lines and favourites entry by entry.
fn merge_text(input: &Input, pending: &HashSet<String>, out: &mut Plan) {
    let (client, hub, base) = (input.client, input.hub, input.base);
    let removed_stems: HashSet<String> = input.tombstones.iter().map(|g| fold(&g.stem)).collect();

    let alive: HashSet<String> = client
        .games
        .iter()
        .chain(&hub.games)
        .filter(|g| !pending.contains(&g.folded()))
        .map(|g| fold(&g.stem))
        .collect();

    // Shell lines, by folded stem: each side's spelling and value.
    let by_stem = |m: &'_ Manifest| -> BTreeMap<String, (String, String)> {
        m.shells
            .iter()
            .map(|(stem, value)| (fold(stem), (stem.clone(), value.clone())))
            .collect()
    };
    let (c, h) = (by_stem(client), by_stem(hub));
    let mut stems: Vec<String> = c.keys().chain(h.keys()).cloned().collect();
    stems.extend(
        base.keys()
            .filter_map(|k| k.strip_prefix("shell:").map(str::to_string)),
    );
    stems.sort();
    stems.dedup();
    for stem in stems {
        let key = format!("shell:{stem}");
        let b = base.get(&key).map(String::as_str);
        let cv = c.get(&stem).map(|(_, v)| v.as_str());
        let hv = h.get(&stem).map(|(_, v)| v.as_str());
        // A removed game's line goes from both sides, unless a game that
        // stays shares the stem (lines are keyed by stem alone).
        let result = if removed_stems.contains(&stem) && !alive.contains(&stem) {
            None
        } else {
            merge(b, cv, hv)
        };
        let spelled = |own: Option<&(String, String)>, other: Option<&(String, String)>| {
            own.or(other).map_or(stem.clone(), |(s, _)| s.clone())
        };
        for (edits, have, own, other) in [
            (&mut out.hub_text, hv, h.get(&stem), c.get(&stem)),
            (&mut out.client_text, cv, c.get(&stem), h.get(&stem)),
        ] {
            if result != have {
                match result {
                    Some(v) => edits.shell_set.push((spelled(own, other), v.to_string())),
                    None => edits.shell_remove.push(spelled(own, other)),
                }
            }
        }
        if result != b {
            out.text_records.push((key, result.map(str::to_string)));
        }
    }

    // Favourites, as a set of games.
    let by_game = |m: &Manifest| -> BTreeMap<String, GameId> {
        m.favourites
            .iter()
            .map(|g| (g.folded(), g.clone()))
            .collect()
    };
    let (c, h) = (by_game(client), by_game(hub));
    let mut games: Vec<String> = c.keys().chain(h.keys()).cloned().collect();
    games.extend(
        base.keys()
            .filter_map(|k| k.strip_prefix("fav:").map(str::to_string)),
    );
    games.sort();
    games.dedup();
    for game in games {
        let key = format!("fav:{game}");
        let b = base.get(&key).map(|_| "1");
        let cv = c.get(&game).map(|_| "1");
        let hv = h.get(&game).map(|_| "1");
        // A removed game is nobody's favourite.
        let result = if pending.contains(&game) {
            None
        } else {
            merge(b, cv, hv)
        };
        let Some(id) = c
            .get(&game)
            .or(h.get(&game))
            .cloned()
            .or_else(|| GameId::parse(&game))
        else {
            continue;
        };
        for (edits, have, own) in [
            (&mut out.hub_text, hv, h.get(&game)),
            (&mut out.client_text, cv, c.get(&game)),
        ] {
            if result != have {
                let id = own.unwrap_or(&id).clone();
                match result {
                    Some(_) => edits.fav_add.push(id),
                    None => edits.fav_remove.push(id),
                }
            }
        }
        if result != b {
            out.text_records.push((key, result.map(str::to_string)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::NameRules;
    use slot_mini_store::Platform;

    fn h(s: &str) -> String {
        crate::hash_bytes(s.as_bytes())
    }

    fn item(key: &str, content: &str) -> Item {
        let key = Key::parse(key).unwrap();
        Item {
            played: (key.kind == Kind::Save).then_some(1000),
            key,
            size: content.len() as u64,
            hash: h(content),
        }
    }

    fn side(items: &[(&str, &str)], games: &[&str]) -> Manifest {
        Manifest {
            now: 2000,
            games: games.iter().map(|g| GameId::parse(g).unwrap()).collect(),
            items: items.iter().map(|(k, c)| item(k, c)).collect(),
            ..Manifest::empty(NameRules::Any)
        }
    }

    fn base(entries: &[(&str, &str)]) -> Base {
        entries
            .iter()
            .map(|(k, c)| {
                (
                    fold(k),
                    if k.starts_with("shell:") || k.starts_with("fav:") {
                        c.to_string()
                    } else {
                        h(c)
                    },
                )
            })
            .collect()
    }

    fn run(scope: Scope, client: &Manifest, hub: &Manifest, base: &Base) -> Plan {
        plan(&Input {
            scope: &scope,
            client,
            hub,
            base,
            tombstones: &[],
            busy: None,
        })
    }

    fn summary(plan: &Plan) -> Vec<String> {
        plan.actions
            .iter()
            .map(|op| match op {
                Op::Remove { game } => format!("remove {game}"),
                Op::Pull { key, new, .. } => {
                    format!("pull {key}{}", if *new { " new" } else { "" })
                }
                Op::Push { key, new, .. } => {
                    format!("push {key}{}", if *new { " new" } else { "" })
                }
                Op::Conflict { key, winner, .. } => format!("conflict {key} {}", winner.as_str()),
            })
            .collect()
    }

    #[test]
    fn the_truth_table() {
        let (b, x, y) = (Some("b"), Some("x"), Some("y"));
        let rows = [
            (None, None, None, Row::Nothing), // 1
            (None, x, None, Row::Push),       // 2
            (None, None, x, Row::Pull),       // 3
            (None, x, x, Row::Record),        // 4
            (None, x, y, Row::Conflict),      // 5
            (b, b, b, Row::Nothing),          // 6
            (b, b, y, Row::Pull),             // 7
            (b, x, b, Row::Push),             // 8
            (b, x, x, Row::Record),           // 9
            (b, x, y, Row::Conflict),         // 10
            (b, None, b, Row::Pull),          // 11
            (b, None, y, Row::Pull),          // 12
            (b, b, None, Row::Push),          // 13
            (b, x, None, Row::Push),          // 14
            (b, None, None, Row::Forget),     // 15
        ];
        for (n, (b, c, h, want)) in rows.into_iter().enumerate() {
            assert_eq!(row(b, c, h), want, "row {}", n + 1);
        }
    }

    #[test]
    fn each_row_becomes_its_action() {
        let k = "label:GBA/X";
        let cases: &[(Option<&str>, Option<&str>, Option<&str>, &str)] = &[
            (None, Some("x"), None, "push label:GBA/X new"),
            (None, None, Some("x"), "pull label:GBA/X new"),
            (None, Some("x"), Some("x"), "record x"),
            (None, Some("x"), Some("y"), "pull label:GBA/X"),
            (Some("b"), Some("b"), Some("b"), ""),
            (Some("b"), Some("b"), Some("y"), "pull label:GBA/X"),
            (Some("b"), Some("x"), Some("b"), "push label:GBA/X"),
            (Some("b"), Some("x"), Some("x"), "record x"),
            (Some("b"), Some("x"), Some("y"), "pull label:GBA/X"),
            (Some("b"), None, Some("b"), "pull label:GBA/X new"),
            (Some("b"), None, Some("y"), "pull label:GBA/X new"),
            (Some("b"), Some("b"), None, "push label:GBA/X new"),
            (Some("b"), Some("x"), None, "push label:GBA/X new"),
            (Some("b"), None, None, "forget"),
        ];
        for (b, c, hub, want) in cases {
            fn one<'a>(k: &'a str, v: &Option<&'a str>) -> Vec<(&'a str, &'a str)> {
                v.map(|v| vec![(k, v)]).unwrap_or_default()
            }
            let list = |v| one(k, v);
            let p = run(
                Scope::Full,
                &side(&list(c), &[]),
                &side(&list(hub), &[]),
                &base(&list(b)),
            );
            let got = match p.records.first() {
                Some((_, Some(hash))) if *hash == h("x") => "record x".to_string(),
                Some((_, None)) => "forget".to_string(),
                Some(other) => format!("{other:?}"),
                None => summary(&p).join(", "),
            };
            assert_eq!(&got, want, "{b:?} {c:?} {hub:?}");
        }
    }

    #[test]
    fn a_save_conflict_goes_to_whoever_played_last() {
        let games = ["GBA/X"];
        let mut client = side(
            &[("save:GBA/X.srm", "mine"), ("save:GBA/X.rtc", "c")],
            &games,
        );
        let mut hub = side(
            &[("save:GBA/x.srm", "theirs"), ("save:GBA/x.rtc", "h")],
            &games,
        );
        let none = Base::new();
        // The same age: the PC.
        let p = run(Scope::Auto, &client, &hub, &none);
        assert_eq!(
            summary(&p),
            ["conflict save:GBA/X.rtc hub", "conflict save:GBA/X.srm hub"]
        );
        assert!(matches!(&p.actions[1], Op::Conflict { hash, mine, .. }
            if *hash == h("theirs") && *mine == h("mine")));
        // The Miyoo played later; its clock is a year off, which cancels.
        client.now += 31_536_000;
        client.items[0].played = Some(client.now - 10);
        // The .rtc was written long ago; it still follows the .srm.
        client.items[1].played = Some(0);
        hub.items[0].played = Some(hub.now - 500);
        let p = run(Scope::Auto, &client, &hub, &none);
        assert_eq!(
            summary(&p),
            [
                "conflict save:GBA/X.rtc client",
                "conflict save:GBA/X.srm client"
            ]
        );
        // No hint: the PC.
        client.items[0].played = None;
        let p = run(Scope::Auto, &client, &hub, &none);
        assert_eq!(summary(&p)[1], "conflict save:GBA/X.srm hub");
    }

    #[test]
    fn saves_wait_for_their_game() {
        let client = side(
            &[("rom:GBA/X.gba", "rom"), ("save:GBA/X.srm", "save")],
            &["GBA/X"],
        );
        let hub = side(&[], &[]);
        // Auto moves no ROMs, so the save has nowhere to go.
        assert!(summary(&run(Scope::Auto, &client, &hub, &Base::new())).is_empty());
        // Full sends the ROM, and the save before it.
        assert_eq!(
            summary(&run(Scope::Full, &client, &hub, &Base::new())),
            ["push save:GBA/X.srm new", "push rom:GBA/X.gba new"]
        );
        // The other way, a ROM that does not fit takes its save with it.
        let mut small = side(&[], &[]);
        small.free = Some(RESERVE + 2);
        let p = run(Scope::Full, &small, &client, &Base::new());
        assert!(summary(&p).is_empty());
        assert_eq!(p.notes.didnt_fit.len(), 1);
    }

    #[test]
    fn a_rom_that_does_not_fit_is_left_out_and_smaller_ones_tried() {
        let hub = side(
            &[
                ("rom:GBA/A big one.gba", "0123456789"),
                ("rom:GBA/B.gba", "0123"),
                ("rom:GBA/C.gba", "01234"),
                ("label:GBA/B", "01"),
            ],
            &["GBA/A big one", "GBA/B", "GBA/C"],
        );
        let mut client = side(&[], &[]);
        client.free = Some(RESERVE + 7);
        let p = run(Scope::Full, &client, &hub, &Base::new());
        // The label's 2 bytes come off first; B fits in the 5 left, C not.
        assert_eq!(
            summary(&p),
            ["pull label:GBA/B new", "pull rom:GBA/B.gba new"]
        );
        let left: Vec<String> = p
            .notes
            .didnt_fit
            .iter()
            .map(|(k, n)| format!("{k} {n}"))
            .collect();
        assert_eq!(left, ["rom:GBA/A big one.gba 10", "rom:GBA/C.gba 5"]);
        // Unknown free space plans everything.
        client.free = None;
        assert_eq!(
            run(Scope::Full, &client, &hub, &Base::new()).actions.len(),
            4
        );
    }

    #[test]
    fn names_a_side_cannot_store_are_listed_and_skipped() {
        let mut client = side(&[("label:GBA/ok", "l")], &[]);
        client.names = NameRules::Fat;
        let hub = side(
            &[("rom:GBA/What?.gba", "rom"), ("rom:GBA/Fine.gba", "rom")],
            &["GBA/What?", "GBA/Fine"],
        );
        let p = run(Scope::Full, &client, &hub, &Base::new());
        assert_eq!(
            summary(&p),
            ["push label:GBA/ok new", "pull rom:GBA/Fine.gba new"]
        );
        assert_eq!(
            p.notes.unstorable,
            [(Key::parse("rom:GBA/What?.gba").unwrap(), Side::Client)]
        );
        // And the other way.
        let p = run(Scope::Full, &hub, &client, &Base::new());
        assert_eq!(p.notes.unstorable[0].1, Side::Hub);
    }

    #[test]
    fn keys_meet_whatever_their_case_and_each_side_keeps_its_spelling() {
        let client = side(&[("label:GBA/GAME", "old")], &[]);
        let hub = side(&[("label:GBA/game", "new")], &[]);
        let p = run(
            Scope::Auto,
            &client,
            &hub,
            &base(&[("label:GBA/Game", "old")]),
        );
        assert_eq!(summary(&p), ["pull label:GBA/GAME"]);
    }

    #[test]
    fn tombstones_remove_and_busy_games_wait() {
        let games = ["GBA/Gone", "GBA/Running", "GBA/Other"];
        let mut client = side(
            &[
                ("label:GBA/Gone", "l"),
                ("label:GBA/Running", "l"),
                ("label:GBA/Other", "l"),
            ],
            &games,
        );
        client.favourites = vec![GameId::parse("GBA/Gone").unwrap()];
        client.shells = vec![("Gone".into(), "auto".into())];
        let hub = side(&[], &[]);
        let scope = Scope::Auto;
        let none = Base::new();
        let mut input = Input {
            scope: &scope,
            client: &client,
            hub: &hub,
            base: &none,
            tombstones: &[GameId::new(Platform::Gba, "gone")],
            busy: Some(&GameId::new(Platform::Gba, "RUNNING")),
        };
        let p = plan(&input);
        assert_eq!(summary(&p), ["remove GBA/gone", "push label:GBA/Other new"]);
        assert_eq!(p.notes.busy.len(), 1);
        // The removed game's shell line and favourite go with it.
        assert!(p.hub_text.is_empty());
        assert_eq!(p.client_text.shell_remove, ["Gone"]);
        assert_eq!(
            p.client_text.fav_remove,
            [GameId::parse("GBA/Gone").unwrap()]
        );
        // A single game's sync does neither.
        let scope = Scope::Game(GameId::new(Platform::Gba, "Gone"));
        input.scope = &scope;
        assert_eq!(summary(&plan(&input)), ["push label:GBA/Gone new"]);
    }

    #[test]
    fn the_plan_is_ordered_removes_small_files_then_roms() {
        let hub = side(
            &[
                ("rom:GB/A.gb", "r"),
                ("save:GBA/Z.srm", "s"),
                ("art:GBA/Z", "a"),
                ("label:GBC/B", "l"),
                ("rom:GBA/Z.gba", "r"),
            ],
            &["GB/A", "GBA/Z"],
        );
        let scope = Scope::Full;
        let p = plan(&Input {
            scope: &scope,
            client: &side(&[], &[]),
            hub: &hub,
            base: &Base::new(),
            tombstones: &[GameId::new(Platform::Gb, "Old")],
            busy: None,
        });
        assert_eq!(
            summary(&p),
            [
                "remove GB/Old",
                "pull label:GBC/B new",
                "pull art:GBA/Z new",
                "pull save:GBA/Z.srm new",
                "pull rom:GB/A.gb new",
                "pull rom:GBA/Z.gba new",
            ]
        );
        let wire = p.to_json();
        for (id, op) in p.actions.iter().enumerate() {
            assert_eq!(wire["actions"][id]["id"], id);
            assert_eq!(Op::from_json(&wire["actions"][id]).as_ref(), Some(op));
        }
    }

    #[test]
    fn text_entries_merge_three_ways_and_the_pc_wins() {
        let (v, w, b) = (Some("v"), Some("w"), Some("b"));
        for (base, c, h, want) in [
            (None, v, v, v),
            (b, v, v, v),
            (None, v, None, v),
            (None, None, v, v),
            (None, v, w, w),
            (b, b, w, w),
            (b, b, None, None),
            (b, v, b, v),
            (b, None, b, None),
            (b, v, w, w),
            (b, v, None, None),
            (b, None, w, w),
            (b, None, None, None),
        ] {
            assert_eq!(merge(base, c, h), want, "{base:?} {c:?} {h:?}");
        }
    }

    #[test]
    fn shells_and_favourites_become_edits_for_each_side() {
        let g = |s: &str| GameId::parse(s).unwrap();
        let mut client = side(&[], &[]);
        client.shells = vec![
            ("Changed here".into(), "auto".into()),
            ("BOTH".into(), "notched 111111 clear".into()),
        ];
        client.favourites = vec![g("GBA/New here"), g("GBA/Dropped there")];
        let mut hub = side(&[], &[]);
        hub.shells = vec![
            ("Changed here".into(), "notched 000000 clear".into()),
            ("both".into(), "notched 222222 clear".into()),
            ("Removed here".into(), "auto".into()),
        ];
        hub.favourites = vec![g("GB/Dropped here"), g("GB/New there")];
        let base = base(&[
            ("shell:Changed here", "notched 000000 clear"),
            ("shell:Removed here", "auto"),
            ("fav:GBA/Dropped there", "1"),
            ("fav:GB/Dropped here", "1"),
            ("fav:GB/Gone from both", "1"),
        ]);
        let p = run(Scope::Auto, &client, &hub, &base);
        assert_eq!(
            p.hub_text,
            TextEdits {
                shell_set: vec![("Changed here".into(), "auto".into())],
                shell_remove: vec!["Removed here".into()],
                fav_add: vec![g("GBA/New here")],
                fav_remove: vec![g("GB/Dropped here")],
            }
        );
        assert_eq!(
            p.client_text,
            TextEdits {
                // Its own spelling, the PC's value.
                shell_set: vec![("BOTH".into(), "notched 222222 clear".into())],
                shell_remove: vec![],
                fav_add: vec![g("GB/New there")],
                fav_remove: vec![g("GBA/Dropped there")],
            }
        );
        assert_eq!(
            TextEdits::from_json(&p.client_text.to_json()),
            p.client_text
        );
        let mut records = p.text_records.clone();
        records.sort();
        let want: Vec<(String, Option<String>)> = [
            ("fav:gb/dropped here", None),
            ("fav:gb/gone from both", None),
            ("fav:gb/new there", Some("1")),
            ("fav:gba/dropped there", None),
            ("fav:gba/new here", Some("1")),
            ("shell:both", Some("notched 222222 clear")),
            ("shell:changed here", Some("auto")),
            ("shell:removed here", None),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.map(str::to_string)))
        .collect();
        assert_eq!(records, want);
        // A single game's sync leaves text alone.
        let p = run(Scope::Game(g("GBA/X")), &client, &hub, &base);
        assert!(p.hub_text.is_empty() && p.text_records.is_empty());
    }
}
