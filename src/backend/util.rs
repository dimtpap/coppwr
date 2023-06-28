// Copyright 2023 Dimitris Papaioannou <dimtpap@protonmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 3 as published by
// the Free Software Foundation.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeMap, ffi::CString, ptr::NonNull};

use pipewire as pw;
use pipewire::spa::{ForeignDict, ReadableDict};

pub fn dict_to_map(dict: &ForeignDict) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (k, v) in dict.iter() {
        map.insert(k.to_string(), v.to_string());
    }
    map
}

pub fn key_val_to_props(
    kv: impl Iterator<Item = (impl Into<Vec<u8>>, impl Into<Vec<u8>>)>,
) -> pw::Properties {
    unsafe {
        use std::ops::Not;

        let raw = NonNull::new(pw::sys::pw_properties_new(std::ptr::null())).unwrap();
        for (k, v) in kv.filter_map(|(k, v)| {
            let k = k.into();
            k.is_empty()
                .not()
                .then(|| (CString::new(k).unwrap(), CString::new(v).unwrap()))
        }) {
            pw::sys::pw_properties_set(raw.as_ptr(), k.as_ptr(), v.as_ptr());
        }
        pw::Properties::from_ptr(raw)
    }
}
