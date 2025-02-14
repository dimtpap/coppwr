// Copyright 2023-2025 Dimitris Papaioannou <dimtpap@protonmail.com>
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

use pipewire::{self as pw, spa::utils::dict::DictRef};

pub fn dict_to_map<'a, K, V>(dict: &'a DictRef) -> BTreeMap<K, V>
where
    K: From<&'a str> + Ord,
    V: From<&'a str>,
{
    BTreeMap::from_iter(dict.iter().map(|(k, v)| (k.into(), v.into())))
}

pub fn key_val_to_props(
    kv: impl Iterator<Item = (impl Into<Vec<u8>>, impl Into<Vec<u8>>)>,
) -> pw::properties::Properties {
    let mut props = pw::properties::Properties::new();
    for (k, v) in kv {
        props.insert(k, v);
    }
    props
}

pub fn connect_override_env(
    context: &pw::context::Context,
    mut context_properties: pw::properties::Properties,
    remote_name: String,
) -> Result<pw::core::Core, pw::Error> {
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
