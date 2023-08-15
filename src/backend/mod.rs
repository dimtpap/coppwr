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
mod pipewire;
pub mod pods;
mod util;

use std::{collections::BTreeMap, sync::mpsc};

#[cfg(feature = "pw_v0_3_77")]
use std::sync::OnceLock;

pub use ::pipewire as pw;
use pw::{permissions::Permissions, types::ObjectType};

use self::pods::profiler::Profiling;

pub enum ObjectMethod {
    ClientGetPermissions {
        index: u32,
        num: u32,
    },
    ClientUpdatePermissions(Vec<Permissions>),
    ClientUpdateProperties(BTreeMap<String, String>),
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
    CreateObject(ObjectType, String, Vec<(String, String)>),
    DestroyObject(u32),
    LoadModule {
        module_dir: Option<String>,
        name: String,
        args: Option<String>,
        props: Option<Vec<(String, String)>>,
    },
    CallObjectMethod(u32, ObjectMethod),
}

pub enum Event {
    GlobalAdded(u32, ObjectType, Option<BTreeMap<String, String>>),
    GlobalRemoved(u32),
    GlobalInfo(u32, Box<[(&'static str, String)]>),
    GlobalProperties(u32, BTreeMap<String, String>),
    ClientPermissions(u32, u32, Vec<Permissions>),
    ProfilerProfile(Vec<Profiling>),
    MetadataProperty {
        id: u32,
        subject: u32,
        key: Option<String>,
        type_: Option<String>,
        value: Option<String>,
    },
    Stop,
}

#[cfg(feature = "pw_v0_3_77")]
static REMOTE_VERSION: OnceLock<(u32, u32, u32)> = OnceLock::new();
#[cfg(feature = "pw_v0_3_77")]
pub fn remote_version<'a>() -> Option<&'a (u32, u32, u32)> {
    REMOTE_VERSION.get()
}

pub fn run(
    remote: String,
) -> (
    std::thread::JoinHandle<()>,
    mpsc::Receiver<Event>,
    pw::channel::Sender<Request>,
) {
    let (sx, rx) = mpsc::channel::<Event>();
    let (pwsx, pwrx) = pw::channel::channel::<Request>();

    (
        std::thread::spawn(move || self::pipewire::pipewire_thread(remote.as_str(), sx, pwrx)),
        rx,
        pwsx,
    )
}
