// Copyright 2023-2024 Dimitris Papaioannou <dimtpap@protonmail.com>
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

use std::collections::BTreeMap;

use pipewire::{
    self as pw,
    spa::{ReadableDict, WritableDict},
};

pub fn dict_to_map<'a, K, V>(dict: &'a impl ReadableDict) -> BTreeMap<K, V>
where
    K: From<&'a str> + Ord,
    V: From<&'a str>,
{
    let mut map = BTreeMap::new();
    for (k, v) in dict.iter() {
        map.insert(k.into(), v.into());
    }
    map
}

pub fn key_val_to_props(
    kv: impl Iterator<Item = (impl Into<Vec<u8>>, impl Into<Vec<u8>>)>,
) -> pw::Properties {
    let mut props = pw::Properties::new();
    for (k, v) in kv {
        props.insert(k, v);
    }
    props
}

pub fn connect_override_env(
    context: &pw::Context,
    mut context_properties: pw::Properties,
    remote_name: String,
) -> Result<pw::Core, pw::Error> {
    let env_remote = std::env::var_os("PIPEWIRE_REMOTE");
    if env_remote.is_some() {
        std::env::remove_var("PIPEWIRE_REMOTE");
    }

    context_properties.insert("remote.name", remote_name);

    let core = context.connect(Some(context_properties))?;

    if let Some(env_remote) = env_remote {
        std::env::set_var("PIPEWIRE_REMOTE", env_remote);
    }

    Ok(core)
}
