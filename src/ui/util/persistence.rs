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

/// Trait for views that would like to save some state between reconnections  
pub trait PersistentView {
    #[cfg(feature = "persistence")]
    type Data: serde::Serialize + serde::de::DeserializeOwned;

    #[cfg(not(feature = "persistence"))]
    type Data;

    fn with_data(data: &Self::Data) -> Self;
    fn save_data(&self) -> Option<Self::Data>;
}
