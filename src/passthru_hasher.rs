/*
 * SPDX-FileCopyrightText: 2023 Nextcloud GmbH and Nextcloud contributors
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */
 
use std::hash::{BuildHasherDefault, Hasher};

pub type PassthruHasher = BuildHasherDefault<Passthru>;

#[derive(Default)]
pub struct Passthru {
    hash: u64,
}

impl Hasher for Passthru {
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, _bytes: &[u8]) {
        panic!("unsupported operation");
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.hash = i;
    }
}
