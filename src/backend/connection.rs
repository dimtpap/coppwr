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

#[cfg(feature = "xdg_desktop_portals")]
use ashpd::desktop::Session;

use pipewire as pw;

use super::{util, RemoteInfo};

#[cfg(feature = "xdg_desktop_portals")]
use super::util::portals;

pub enum Error {
    PipeWire(pw::Error),

    #[cfg(feature = "xdg_desktop_portals")]
    MissingFd,
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

#[cfg(not(feature = "xdg_desktop_portals"))]
pub struct Connection(pw::Core);
#[cfg(not(feature = "xdg_desktop_portals"))]
impl Connection {
    pub fn connect(context: &pw::Context<pw::MainLoop>, remote: RemoteInfo) -> Result<Self, Error> {
        let RemoteInfo::Regular(remote) = remote;
        Ok(Self(util::manager_core(context, remote.as_str())?))
    }

    pub fn core(&self) -> &pw::Core {
        &self.0
    }
}

#[cfg(feature = "xdg_desktop_portals")]
pub enum Connection<'s> {
    Simple(pw::Core),
    PortalWithSession(pw::Core, Session<'s>),
}

#[cfg(feature = "xdg_desktop_portals")]
impl<'s> Connection<'s> {
    pub fn connect(context: &pw::Context<pw::MainLoop>, remote: RemoteInfo) -> Result<Self, Error> {
        match remote {
            RemoteInfo::Regular(name) => {
                Ok(Self::Simple(util::manager_core(context, name.as_str())?))
            }
            RemoteInfo::Screencast { types, multiple } => {
                let (fd, session) = portals::open_screencast_remote(types, multiple)?;

                Ok(Self::PortalWithSession(
                    util::manager_core_fd(context, fd)?,
                    session,
                ))
            }
            RemoteInfo::Camera => Ok(Self::Simple(util::manager_core_fd(
                context,
                portals::open_camera_remote()?.ok_or(Error::MissingFd)?,
            )?)),
        }
    }

    pub fn core(&self) -> &pw::Core {
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