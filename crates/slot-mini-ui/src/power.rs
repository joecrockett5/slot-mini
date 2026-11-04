// Copied from slot (https://github.com/BrandonKowalski/slot) at e66715d:
//   crates/slot-power/src/lib.rs (`Charge`, `Battery` only)
// Changes: the rest of slot-power (sysfs readers for the RG SP) is not used;
//   slot-mini reads Onion's /tmp/percBat instead.
// Copyright (C) Brandon Kowalski and slot contributors. GPL-3.0-or-later.

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Charge {
    Unknown,
    Discharging,
    Charging,
    Full,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Battery {
    pub percent: u8,
    pub charge: Charge,
}
