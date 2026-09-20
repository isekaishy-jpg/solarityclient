//! Indexed lookup over the original ordered attachment samples.

use glam::Mat4;
use solarity_rendering::CharacterAttachmentPoint;
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Deref;

/// A sampled parent identity and its authored optional attachment transform.
pub(in crate::application::terrain_frame::m2) trait AttachmentSample:
    Copy
{
    type Key: Copy + Eq + Hash;

    fn key(self) -> Self::Key;
    fn transform(self) -> Option<Mat4>;
}

impl AttachmentSample for (u64, Option<Mat4>) {
    type Key = u64;
    fn key(self) -> Self::Key {
        self.0
    }
    fn transform(self) -> Option<Mat4> {
        self.1
    }
}

impl AttachmentSample for (u64, CharacterAttachmentPoint, Option<Mat4>) {
    type Key = (u64, CharacterAttachmentPoint);
    fn key(self) -> Self::Key {
        (self.0, self.1)
    }
    fn transform(self) -> Option<Mat4> {
        self.2
    }
}

impl AttachmentSample for (u64, CharacterAttachmentPoint, u32, Option<Mat4>) {
    type Key = (u64, CharacterAttachmentPoint, u32);
    fn key(self) -> Self::Key {
        (self.0, self.1, self.2)
    }
    fn transform(self) -> Option<Mat4> {
        self.3
    }
}

/// Duplicate parents keep their first result, but any hidden rider hides items.
struct PublishedParent {
    first: usize,
    any_hidden: bool,
}

/// Writes maintain the index; the slice view intentionally exposes only reads.
pub(in crate::application::terrain_frame::m2) struct AttachmentSamples<S: AttachmentSample> {
    ordered: Vec<S>,
    parents: HashMap<S::Key, PublishedParent>,
}

impl<S: AttachmentSample> Default for AttachmentSamples<S> {
    fn default() -> Self {
        Self {
            ordered: Vec::new(),
            parents: HashMap::new(),
        }
    }
}

impl<S: AttachmentSample> Deref for AttachmentSamples<S> {
    type Target = [S];

    fn deref(&self) -> &[S] {
        &self.ordered
    }
}

impl<S: AttachmentSample> AttachmentSamples<S> {
    /// Starts a new publication epoch; neither records nor hidden flags survive.
    pub(in crate::application::terrain_frame::m2) fn clear(&mut self) {
        self.ordered.clear();
        self.parents.clear();
    }

    /// Reserves additional records and worst-case distinct parents together.
    pub(in crate::application::terrain_frame::m2) fn reserve(&mut self, additional: usize) {
        self.ordered.reserve(additional);
        self.parents.reserve(additional);
    }

    /// Publishes one original record without collapsing duplicate identities.
    pub(in crate::application::terrain_frame::m2) fn push(&mut self, sample: S) {
        let parent = self.parents.entry(sample.key()).or_insert(PublishedParent {
            first: self.ordered.len(),
            any_hidden: false,
        });
        parent.any_hidden |= sample.transform().is_none();
        self.ordered.push(sample);
    }

    /// Matches the former ordered `find_map`, including a first hidden sample.
    pub(in crate::application::terrain_frame::m2) fn first(
        &self,
        key: S::Key,
    ) -> Option<Option<Mat4>> {
        self.parents
            .get(&key)
            .map(|parent| self.ordered[parent.first].transform())
    }

    /// Matches the rider scan for any hidden sample, independently of the first.
    pub(in crate::application::terrain_frame::m2) fn any_hidden(&self, key: S::Key) -> bool {
        self.parents
            .get(&key)
            .is_some_and(|parent| parent.any_hidden)
    }
}
