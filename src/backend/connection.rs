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

#[cfg(feature = "xdg_desktop_portals")]
use ashpd::desktop::Session;

use pipewire as pw;

use super::{util, RemoteInfo};

#[derive(Debug)]
pub enum Error {
    PipeWire(pw::Error),

    #[cfg(feature = "xdg_desktop_portals")]
    PortalUnavailable,
    #[cfg(feature = "xdg_desktop_portals")]
    Ashpd(ashpd::Error),
}

impl From<pw::Error> for Error {
    fn from(value: pw::Error) -> Self {
        Self::PipeWire(value)
    }
}

#[cfg(feature = "xdg_desktop_portals")]
impl From<ashpd::Error> for Error {
    fn from(value: ashpd::Error) -> Self {
        Self::Ashpd(value)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PipeWire(e) => f.write_fmt(format_args!("Connecting to PipeWire failed: {e}")),

            #[cfg(feature = "xdg_desktop_portals")]
            Self::PortalUnavailable => f.write_str("Portal is unavailable"),

            #[cfg(feature = "xdg_desktop_portals")]
            Self::Ashpd(e) => f.write_fmt(format_args!("Connecting to the portal failed: {e}")),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(feature = "xdg_desktop_portals")]
mod portals {
    use std::os::fd::OwnedFd;

    use ashpd::{
        desktop::{screencast::SourceType, Session},
        enumflags2::BitFlags,
    };

    pub fn open_screencast_remote<'s>(
        types: BitFlags<SourceType>,
        multiple: bool,
    ) -> Result<(OwnedFd, Session<'s>), ashpd::Error> {
        pollster::block_on(async {
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

            Ok((fd, session))
        })
    }

    pub fn open_camera_remote() -> Result<Option<OwnedFd>, ashpd::Error> {
        pollster::block_on(ashpd::desktop::camera::request())
    }
}

#[cfg(not(feature = "xdg_desktop_portals"))]
pub struct Connection(pw::core::Core);
#[cfg(not(feature = "xdg_desktop_portals"))]
impl Connection {
    pub fn connect(
        context: &pw::context::Context,
        context_properties: Vec<(String, String)>,
        remote: RemoteInfo,
    ) -> Result<Self, Error> {
        let RemoteInfo::Regular(remote) = remote;
        Ok(Self(util::connect_override_env(
            context,
            util::key_val_to_props(context_properties.into_iter()),
            remote,
        )?))
    }

    pub const fn core(&self) -> &pw::core::Core {
        &self.0
    }
}

#[cfg(feature = "xdg_desktop_portals")]
pub enum Connection<'s> {
    Simple(pw::core::Core),
    PortalWithSession(pw::core::Core, Session<'s>),
}

#[cfg(feature = "xdg_desktop_portals")]
impl<'s> Connection<'s> {
    pub fn connect(
        context: &pw::context::Context,
        context_properties: Vec<(String, String)>,
        remote: RemoteInfo,
    ) -> Result<Self, Error> {
        let context_properties = util::key_val_to_props(context_properties.into_iter());

        match remote {
            RemoteInfo::Regular(remote_name) => Ok(Self::Simple(util::connect_override_env(
                context,
                context_properties,
                remote_name,
            )?)),
            RemoteInfo::Screencast { types, multiple } => {
                let (fd, session) = portals::open_screencast_remote(types, multiple)?;

                Ok(Self::PortalWithSession(
                    context.connect_fd(fd, Some(context_properties))?,
                    session,
                ))
            }
            RemoteInfo::Camera => Ok(Self::Simple(context.connect_fd(
                portals::open_camera_remote()?.ok_or(Error::PortalUnavailable)?,
                Some(context_properties),
            )?)),
        }
    }

    pub const fn core(&self) -> &pw::core::Core {
        match self {
            Self::Simple(core) | Self::PortalWithSession(core, _) => core,
        }
    }
}

#[cfg(feature = "xdg_desktop_portals")]
impl<'s> Drop for Connection<'s> {
    fn drop(&mut self) {
        if let Self::PortalWithSession(_, session) = self {
            if let Err(e) = pollster::block_on(session.close()) {
                eprintln!("Error when stopping portal session: {e}");
            }
        }
    }
}
