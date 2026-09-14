//! Simulation access preserves the structural journal owned by storage.

use std::ops::{Index, IndexMut};
use std::slice::SliceIndex;

use super::{M2GpuPlacement, M2PlacementStorage};

impl M2PlacementStorage {
    pub(in super::super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(in super::super) fn as_slice(&self) -> &[M2GpuPlacement] {
        &self.entries
    }

    pub(in super::super) fn as_mut_slice(&mut self) -> &mut [M2GpuPlacement] {
        &mut self.entries
    }

    pub(in super::super) fn iter(&self) -> std::slice::Iter<'_, M2GpuPlacement> {
        self.entries.iter()
    }

    pub(in super::super) fn iter_mut(&mut self) -> std::slice::IterMut<'_, M2GpuPlacement> {
        self.entries.iter_mut()
    }
}

impl<I: SliceIndex<[M2GpuPlacement]>> Index<I> for M2PlacementStorage {
    type Output = I::Output;

    fn index(&self, index: I) -> &Self::Output {
        &self.entries[index]
    }
}

impl<I: SliceIndex<[M2GpuPlacement]>> IndexMut<I> for M2PlacementStorage {
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        &mut self.entries[index]
    }
}

impl<'a> IntoIterator for &'a M2PlacementStorage {
    type Item = &'a M2GpuPlacement;
    type IntoIter = std::slice::Iter<'a, M2GpuPlacement>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut M2PlacementStorage {
    type Item = &'a mut M2GpuPlacement;
    type IntoIter = std::slice::IterMut<'a, M2GpuPlacement>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}
