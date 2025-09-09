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

use internment::Intern;

/// An interned string
// This should be drop-in usable with egui and PipeWire
#[derive(Debug, Hash, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Istr(Intern<Box<str>>);

impl std::ops::Deref for Istr {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &*(*self.0)
    }
}

// T -> Istr

impl From<String> for Istr {
    fn from(value: String) -> Self {
        Self(Intern::from_ref(value.as_str()))
    }
}

impl From<&str> for Istr {
    fn from(value: &str) -> Self {
        Self(Intern::from_ref(value))
    }
}

impl AsRef<str> for Istr {
    fn as_ref(&self) -> &str {
        &self
    }
}

// Istr -> T

impl Istr {
    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

impl Into<Vec<u8>> for Istr {
    fn into(self) -> Vec<u8> {
        (*self).into()
    }
}

impl std::borrow::Borrow<str> for Istr {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Into<egui::WidgetText> for Istr {
    fn into(self) -> egui::WidgetText {
        (&self).into()
    }
}

impl Into<egui::WidgetText> for &Istr {
    fn into(self) -> egui::WidgetText {
        self.as_str().into()
    }
}

// Istr == T

impl PartialEq<str> for Istr {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
