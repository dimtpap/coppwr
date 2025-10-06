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

use std::{collections::VecDeque, ops::RangeBounds};

/// A ring buffer that drops the oldest items (those at the front) when filled up
#[derive(Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct RingBuf<T>(VecDeque<T>);

impl<T> RingBuf<T> {
    pub fn new() -> Self {
        Self(VecDeque::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(VecDeque::with_capacity(capacity))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Push an item back while making the buffer hold at most `max` items.
    /// This will resize the buffer according to `max` if necessary.
    pub fn push_back(&mut self, max: usize, value: T) {
        if max == 0 {
            self.clear();
            return;
        }

        if self.len() == max {
            self.0.pop_front();
        } else if self.len() > max {
            self.resize(max - 1);
        }

        self.0.push_back(value)
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }

    pub fn drain(&mut self, range: impl RangeBounds<usize>) -> impl Iterator<Item = T> {
        self.0.drain(range)
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.0.iter()
    }

    pub fn extend(&mut self, max: usize, iter: impl ExactSizeIterator<Item = T>) {
        let mut skip = 0;

        if iter.len() >= max {
            self.clear();
            skip = iter.len() - max;
        } else if self.len() + iter.len() > max {
            _ = self.drain(0..(self.len() + iter.len() - max));
        }

        self.0.extend(iter.skip(skip));
    }

    /// Drop oldest items if shrinking
    /// Reserve memory for more items if growing
    pub fn resize(&mut self, max: usize) {
        if self.0.capacity() < max {
            self.0.reserve(max - self.len());
        } else if self.len() > max {
            self.0.drain(0..self.len() - max);
            self.0.shrink_to_fit();
        }
    }
}

impl<T> IntoIterator for RingBuf<T> {
    type Item = T;
    type IntoIter = <VecDeque<T> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a RingBuf<T> {
    type Item = &'a T;
    type IntoIter = <&'a VecDeque<T> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<T, const N: usize> From<[T; N]> for RingBuf<T> {
    fn from(value: [T; N]) -> Self {
        Self(VecDeque::from(value))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn overflow() {
        let mut rb = RingBuf::with_capacity(2);

        rb.push_back(2, 1);
        rb.push_back(2, 2);
        rb.push_back(2, 3);

        assert_eq!(rb.into_iter().next(), Some(2));
    }

    #[test]
    fn resize() {
        let mut rb = RingBuf::from([1, 2]);

        rb.resize(1);
        assert_eq!(rb.iter().next(), Some(&2));

        rb.resize(0);
        assert_eq!(rb.iter().next(), None);
    }

    #[test]
    fn extend_all_fit() {
        // All items fit
        let mut rb = RingBuf::from([1, 2]);
        let other = [3, 4];
        rb.extend(4, other.into_iter());

        assert_eq!(rb.into_iter().next(), Some(1));
    }

    #[test]
    fn extend_truncate() {
        // rb needs truncation
        let mut rb = RingBuf::from([1, 2]);
        let other = [3, 4];
        rb.extend(3, other.into_iter());

        assert_eq!(rb.into_iter().next(), Some(2));
    }

    #[test]
    fn extend_clear() {
        // rb needs to be cleared
        let mut rb = RingBuf::from([1, 2]);
        let other = [3, 4];
        rb.extend(2, other.into_iter());

        assert_eq!(rb.into_iter().next(), Some(3));
    }

    #[test]
    fn extend_skip() {
        // rb needs to be cleared and other needs to be skipped
        let mut rb = RingBuf::from([1, 2]);
        let other = [3, 4];
        rb.extend(1, other.into_iter());

        assert_eq!(rb.into_iter().next(), Some(4));
    }
}
