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

use pipewire::spa::{pod::deserialize::*, utils::Fraction};

#[derive(Debug)]
pub struct Info {
    pub counter: i64,
    pub cpu_load_fast: f32,
    pub cpu_load_medium: f32,
    pub cpu_load_slow: f32,
    pub xrun_count: i32,
}

impl<'de> PodDeserialize<'de> for Info {
    fn deserialize(
        deserializer: PodDeserializer<'de>,
    ) -> Result<(Self, DeserializeSuccess<'de>), DeserializeError<&'de [u8]>>
    where
        Self: Sized,
    {
        struct InfoVisitor;

        impl<'de> Visitor<'de> for InfoVisitor {
            type Value = Info;
            type ArrayElem = std::convert::Infallible;

            fn visit_struct(
                &self,
                struct_deserializer: &mut StructPodDeserializer<'de>,
            ) -> Result<Self::Value, DeserializeError<&'de [u8]>> {
                Ok(Info {
                    counter: struct_deserializer.deserialize_field()?.unwrap(),
                    cpu_load_fast: struct_deserializer.deserialize_field()?.unwrap(),
                    cpu_load_medium: struct_deserializer.deserialize_field()?.unwrap(),
                    cpu_load_slow: struct_deserializer.deserialize_field()?.unwrap(),
                    xrun_count: struct_deserializer.deserialize_field()?.unwrap(),
                })
            }
        }

        deserializer.deserialize_struct(InfoVisitor)
    }
}

#[derive(Debug)]
pub struct Clock {
    pub flags: i32,
    pub id: i32,
    pub name: String,
    pub nsec: i64,
    pub rate: Fraction,
    pub position: i64,
    pub duration: i64,
    pub delay: i64,
    pub rate_diff: f64,
    pub next_nsec: i64,
}

impl<'de> PodDeserialize<'de> for Clock {
    fn deserialize(
        deserializer: PodDeserializer<'de>,
    ) -> Result<(Self, DeserializeSuccess<'de>), DeserializeError<&'de [u8]>>
    where
        Self: Sized,
    {
        struct ClockVisitor;

        impl<'de> Visitor<'de> for ClockVisitor {
            type Value = Clock;
            type ArrayElem = std::convert::Infallible;

            fn visit_struct(
                &self,
                struct_deserializer: &mut StructPodDeserializer<'de>,
            ) -> Result<Self::Value, DeserializeError<&'de [u8]>> {
                Ok(Clock {
                    flags: struct_deserializer.deserialize_field()?.unwrap(),
                    id: struct_deserializer.deserialize_field()?.unwrap(),
                    name: struct_deserializer.deserialize_field()?.unwrap(),
                    nsec: struct_deserializer.deserialize_field()?.unwrap(),
                    rate: struct_deserializer.deserialize_field()?.unwrap(),
                    position: struct_deserializer.deserialize_field()?.unwrap(),
                    duration: struct_deserializer.deserialize_field()?.unwrap(),
                    delay: struct_deserializer.deserialize_field()?.unwrap(),
                    rate_diff: struct_deserializer.deserialize_field()?.unwrap(),
                    next_nsec: struct_deserializer.deserialize_field()?.unwrap(),
                })
            }
        }

        deserializer.deserialize_struct(ClockVisitor)
    }
}

#[derive(Debug)]
pub struct NodeBlock {
    pub id: i32,
    pub name: String,
    pub prev_signal: i64,
    pub signal: i64,
    pub awake: i64,
    pub finish: i64,
    pub status: i32,
    pub latency: Fraction,
    pub xrun_count: Option<i32>,
}

impl<'de> PodDeserialize<'de> for NodeBlock {
    fn deserialize(
        deserializer: PodDeserializer<'de>,
    ) -> Result<(Self, DeserializeSuccess<'de>), DeserializeError<&'de [u8]>>
    where
        Self: Sized,
    {
        struct NodeBlockVisitor;

        impl<'de> Visitor<'de> for NodeBlockVisitor {
            type Value = NodeBlock;
            type ArrayElem = std::convert::Infallible;

            fn visit_struct(
                &self,
                struct_deserializer: &mut StructPodDeserializer<'de>,
            ) -> Result<Self::Value, DeserializeError<&'de [u8]>> {
                Ok(NodeBlock {
                    id: struct_deserializer.deserialize_field()?.unwrap(),
                    name: struct_deserializer.deserialize_field()?.unwrap(),
                    prev_signal: struct_deserializer.deserialize_field()?.unwrap(),
                    signal: struct_deserializer.deserialize_field()?.unwrap(),
                    awake: struct_deserializer.deserialize_field()?.unwrap(),
                    finish: struct_deserializer.deserialize_field()?.unwrap(),
                    status: struct_deserializer.deserialize_field()?.unwrap(),
                    latency: struct_deserializer.deserialize_field()?.unwrap(),
                    xrun_count: struct_deserializer.deserialize_field()?,
                })
            }
        }

        deserializer.deserialize_struct(NodeBlockVisitor)
    }
}

#[derive(Debug)]
pub struct Profiling {
    pub info: Info,
    pub clock: Clock,
    pub driver: NodeBlock,
    pub followers: Vec<NodeBlock>,
}

impl<'de> PodDeserialize<'de> for Profiling {
    fn deserialize(
        deserializer: PodDeserializer<'de>,
    ) -> Result<(Self, DeserializeSuccess<'de>), DeserializeError<&'de [u8]>>
    where
        Self: Sized,
    {
        struct ProfilingVisitor;

        impl<'de> Visitor<'de> for ProfilingVisitor {
            type Value = Profiling;
            type ArrayElem = std::convert::Infallible;

            fn visit_object(
                &self,
                object_deserializer: &mut ObjectPodDeserializer<'de>,
            ) -> Result<Self::Value, DeserializeError<&'de [u8]>> {
                let info: Info = object_deserializer.deserialize_property()?.unwrap().0;
                let clock: Clock = object_deserializer.deserialize_property()?.unwrap().0;
                let driver: NodeBlock = object_deserializer.deserialize_property()?.unwrap().0;

                let mut followers = Vec::new();

                while let Some((fb, _, _)) = object_deserializer.deserialize_property()? {
                    followers.push(fb);
                }

                Ok(Profiling {
                    info,
                    clock,
                    driver,
                    followers,
                })
            }
        }

        deserializer.deserialize_object(ProfilingVisitor)
    }
}

#[derive(Debug)]
pub struct Profilings(pub Vec<Profiling>);

impl<'de> PodDeserialize<'de> for Profilings {
    fn deserialize(
        deserializer: PodDeserializer<'de>,
    ) -> Result<(Self, DeserializeSuccess<'de>), DeserializeError<&'de [u8]>>
    where
        Self: Sized,
    {
        struct ProfilerVisitor;

        impl<'de> Visitor<'de> for ProfilerVisitor {
            type Value = Profilings;
            type ArrayElem = std::convert::Infallible;

            fn visit_struct(
                &self,
                struct_deserializer: &mut StructPodDeserializer<'de>,
            ) -> Result<Self::Value, DeserializeError<&'de [u8]>> {
                let mut profilings = Vec::new();

                while let Some(p) = struct_deserializer.deserialize_field()? {
                    profilings.push(p);
                }

                Ok(Profilings(profilings))
            }
        }

        deserializer.deserialize_struct(ProfilerVisitor)
    }
}
