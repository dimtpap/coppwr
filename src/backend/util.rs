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

use std::collections::BTreeMap;

use pipewire::{
    self as pw,
    spa::{ForeignDict, ReadableDict, WritableDict},
};

pub fn dict_to_map<'a, K, V>(dict: &'a ForeignDict) -> BTreeMap<K, V>
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

pub fn manager_core(
    context: &pw::Context<pw::MainLoop>,
    remote_name: &str,
) -> Result<pw::Core, pw::Error> {
    let env_remote = std::env::var_os("PIPEWIRE_REMOTE");
    std::env::remove_var("PIPEWIRE_REMOTE");

    let core = context.connect(Some(key_val_to_props(
        [("media.category", "Manager"), ("remote.name", remote_name)].into_iter(),
    )))?;

    if let Some(env_remote) = env_remote {
        std::env::set_var("PIPEWIRE_REMOTE", env_remote);
    }

    Ok(core)
}

#[cfg(feature = "xdg_desktop_portals")]
pub fn manager_core_fd(
    context: &pw::Context<pw::MainLoop>,
    fd: std::os::fd::OwnedFd,
) -> Result<pw::Core, pw::Error> {
    context.connect_fd(
        fd,
        Some(pw::properties! {
            "media.category" => "Manager",
        }),
    )
}

#[cfg(feature = "xdg_desktop_portals")]
pub mod portals {
    use std::os::fd::{FromRawFd, OwnedFd};

    use ashpd::{
        desktop::{screencast::SourceType, Session},
        enumflags2::BitFlags,
    };

    pub fn open_screencast_remote<'s>(
        types: BitFlags<SourceType>,
        multiple: bool,
    ) -> Result<(OwnedFd, Session<'s>), ashpd::Error> {
        async fn async_inner<'s>(
            types: BitFlags<SourceType>,
            multiple: bool,
        ) -> Result<(OwnedFd, Session<'s>), ashpd::Error> {
            use ashpd::desktop::screencast::{CursorMode, PersistMode, Screencast};

            let proxy = Screencast::new().await?;
            let session = proxy.create_session().await?;

            proxy
                .select_sources(
                    &session,
                    CursorMode::Hidden,
                    types,
                    multiple,
                    None,
                    PersistMode::DoNot,
                )
                .await?;

            proxy
                .start(&session, &ashpd::WindowIdentifier::default())
                .await?;

            let fd = proxy.open_pipe_wire_remote(&session).await?;

            Ok((unsafe { OwnedFd::from_raw_fd(fd) }, session))
        }

        pollster::block_on(async_inner(types, multiple))
    }

    pub fn open_camera_remote() -> Result<Option<OwnedFd>, ashpd::Error> {
        pollster::block_on(ashpd::desktop::camera::request())
            .map(|fd| fd.map(|fd| unsafe { OwnedFd::from_raw_fd(fd) }))
    }
}
