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

use pipewire as pw;

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

#[cfg(not(feature = "xdg_desktop_portals"))]
mod connection {
    use pipewire as pw;

    use crate::backend::{util, RemoteInfo};

    use super::Error;

    pub struct Connection(pw::core::Core);

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
}

#[cfg(feature = "xdg_desktop_portals")]
mod connection {
    use std::os::fd::OwnedFd;

    use ashpd::{
        desktop::{
            screencast::{Screencast, SourceType},
            Session,
        },
        enumflags2::BitFlags,
    };
    use pipewire as pw;

    use crate::backend::{util, RemoteInfo};

    use super::Error;

    enum PortalSession<'a, 'b> {
        Screencast(Session<'a, Screencast<'b>>),
    }

    impl PortalSession<'_, '_> {
        fn close(&self) -> Result<(), ashpd::Error> {
            match self {
                Self::Screencast(s) => pollster::block_on(s.close()),
            }
        }
    }

    impl<'a, 'b> From<Session<'a, Screencast<'b>>> for PortalSession<'a, 'b> {
        fn from(value: Session<'a, Screencast<'b>>) -> Self {
            Self::Screencast(value)
        }
    }

    pub struct Connection<'a, 'b> {
        core: pw::core::Core,
        session: Option<PortalSession<'a, 'b>>,
    }

    impl<'a, 'b> Connection<'a, 'b> {
        pub fn connect(
            context: &pw::context::Context,
            context_properties: Vec<(String, String)>,
            remote: RemoteInfo,
        ) -> Result<Self, Error> {
            fn open_screencast_remote<'a, 'b>(
                types: BitFlags<SourceType>,
                multiple: bool,
            ) -> Result<(OwnedFd, Session<'a, Screencast<'b>>), ashpd::Error> {
                pollster::block_on(async {
                    use ashpd::desktop::{
                        screencast::{CursorMode, Screencast},
                        PersistMode,
                    };

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

            fn open_camera_remote() -> Result<Option<OwnedFd>, ashpd::Error> {
                pollster::block_on(ashpd::desktop::camera::request())
            }

            let context_properties = util::key_val_to_props(context_properties.into_iter());

            match remote {
                RemoteInfo::Regular(remote_name) => Ok(Self {
                    core: util::connect_override_env(context, context_properties, remote_name)?,
                    session: None,
                }),
                RemoteInfo::Screencast { types, multiple } => {
                    let (fd, session) = open_screencast_remote(types, multiple)?;

                    Ok(Self {
                        core: context.connect_fd(fd, Some(context_properties))?,
                        session: Some(session.into()),
                    })
                }
                RemoteInfo::Camera => Ok(Self {
                    core: context.connect_fd(
                        open_camera_remote()?.ok_or(Error::PortalUnavailable)?,
                        Some(context_properties),
                    )?,
                    session: None,
                }),
            }
        }

        pub const fn core(&self) -> &pw::core::Core {
            &self.core
        }
    }

    impl Drop for Connection<'_, '_> {
        fn drop(&mut self) {
            if let Some(Err(e)) = self.session.as_ref().map(PortalSession::close) {
                eprintln!("Error when closing portal session: {e}");
            }
        }
    }
}

pub use connection::Connection;
