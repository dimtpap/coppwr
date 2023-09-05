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

mod bind;
mod connection;
mod pipewire;
pub mod pods;
mod util;

use ::pipewire as pw;

use connection::Connection;

pub type Sender = pw::channel::Sender<Request>;

pub enum ObjectMethod {
    ClientGetPermissions {
        index: u32,
        num: u32,
    },
    ClientUpdatePermissions(Vec<pw::permissions::Permissions>),
    ClientUpdateProperties(std::collections::BTreeMap<String, String>),
    MetadataSetProperty {
        subject: u32,
        key: String,
        type_: Option<String>,
        value: Option<String>,
    },
    MetadataClear,
}

pub enum Request {
    Stop,
    CreateObject(pw::types::ObjectType, String, Vec<(String, String)>),
    DestroyObject(u32),
    LoadModule {
        module_dir: Option<String>,
        name: String,
        args: Option<String>,
        props: Option<Vec<(String, String)>>,
    },
    GetContextProperties,
    UpdateContextProperties(std::collections::BTreeMap<String, String>),
    CallObjectMethod(u32, ObjectMethod),
}

pub enum Event {
    GlobalAdded(
        u32,
        pw::types::ObjectType,
        Option<std::collections::BTreeMap<String, String>>,
    ),
    GlobalRemoved(u32),
    GlobalInfo(u32, Box<[(&'static str, String)]>),
    GlobalProperties(u32, std::collections::BTreeMap<String, String>),
    ClientPermissions(u32, u32, Vec<pw::permissions::Permissions>),
    ProfilerProfile(Vec<self::pods::profiler::Profiling>),
    MetadataProperty {
        id: u32,
        subject: u32,
        key: Option<String>,
        type_: Option<String>,
        value: Option<String>,
    },
    ContextProperties(std::collections::BTreeMap<String, String>),
    Stop,
}

#[cfg(feature = "pw_v0_3_77")]
static REMOTE_VERSION: std::sync::OnceLock<(u32, u32, u32)> = std::sync::OnceLock::new();
#[cfg(feature = "pw_v0_3_77")]
pub fn remote_version<'a>() -> Option<&'a (u32, u32, u32)> {
    REMOTE_VERSION.get()
}

pub enum RemoteInfo {
    Regular(String),

    #[cfg(feature = "xdg_desktop_portals")]
    Screencast {
        types: ashpd::enumflags2::BitFlags<ashpd::desktop::screencast::SourceType>,
        multiple: bool,
    },
    #[cfg(feature = "xdg_desktop_portals")]
    Camera,
}

impl PartialEq for RemoteInfo {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

impl Default for RemoteInfo {
    fn default() -> Self {
        Self::Regular(
            std::env::var("PIPEWIRE_REMOTE").unwrap_or_else(|_| String::from("pipewire-0")),
        )
    }
}

pub struct Handle {
    thread: Option<std::thread::JoinHandle<()>>,
    pub rx: std::sync::mpsc::Receiver<Event>,
    pub sx: Sender,
}

impl Handle {
    pub fn run(remote: RemoteInfo, context_properties: Vec<(String, String)>) -> Self {
        let (sx, rx) = std::sync::mpsc::channel::<Event>();
        let (pwsx, pwrx) = pw::channel::channel::<Request>();

        Self {
            thread: Some(std::thread::spawn(move || {
                self::pipewire::pipewire_thread(remote, context_properties, sx, pwrx);
            })),
            rx,
            sx: pwsx,
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        if self.sx.send(Request::Stop).is_err() {
            eprintln!("Error sending stop request to PipeWire");
        }
        if let Some(Err(e)) = self.thread.take().map(std::thread::JoinHandle::join) {
            eprintln!("The PipeWire thread has paniced: {e:?}");
        }
    }
}
