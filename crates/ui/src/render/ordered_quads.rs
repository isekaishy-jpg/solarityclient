//! Stable packet ordering without sorting or moving individual glyph payloads.

use std::iter::Peekable;
use std::ops::Range;
use std::vec::IntoIter;

use crate::{UiGlyphQuad, UiPresentationPacketKey};

/// Matches the original complete-quad stable sort, including ownerless packets.
pub(super) type UiQuadOrder = (bool, Option<UiPresentationPacketKey>, usize, usize);

/// One source index whose payload is converted only when the mesh consumes it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum UiOrderedQuadSlot {
    Texture(usize),
    Glyph(usize),
}

/// Adjacent glyphs share packet and owner; only their sequence component advances.
struct UiGlyphDrawRun {
    next_order: UiQuadOrder,
    range: Range<usize>,
}

/// Exact-size merge of sorted texture entries and sorted contiguous glyph runs.
pub(super) struct UiOrderedQuadSlots {
    textures: Peekable<IntoIter<(UiQuadOrder, usize)>>,
    glyph_runs: IntoIter<UiGlyphDrawRun>,
    active_glyph_run: Option<UiGlyphDrawRun>,
    remaining: usize,
}

impl UiOrderedQuadSlots {
    /// Groups glyphs in their original sequence before sorting the much smaller run list.
    pub(super) fn new(texture_orders: Vec<UiQuadOrder>, glyphs: &[UiGlyphQuad]) -> Self {
        let texture_count = texture_orders.len();
        let mut glyph_runs: Vec<UiGlyphDrawRun> = Vec::new();
        for (index, glyph) in glyphs.iter().enumerate() {
            let packet = glyph.packet_key();
            let owner = glyph.object_index();
            if let Some(run) = glyph_runs.last_mut()
                && run.next_order.1 == packet
                && run.next_order.2 == owner
            {
                run.range.end += 1;
            } else {
                glyph_runs.push(UiGlyphDrawRun {
                    next_order: (packet.is_none(), packet, owner, texture_count + index),
                    range: index..index + 1,
                });
            }
        }
        Self::from_runs(texture_orders, glyph_runs)
    }

    /// Retains the original stable texture-before-glyph rule at equal sequence keys.
    fn from_runs(texture_orders: Vec<UiQuadOrder>, mut glyph_runs: Vec<UiGlyphDrawRun>) -> Self {
        let mut textures = texture_orders
            .into_iter()
            .enumerate()
            .map(|(index, order)| (order, index))
            .collect::<Vec<_>>();
        textures.sort_by_key(|(order, _index)| *order);
        glyph_runs.sort_by_key(|run| run.next_order);
        let remaining =
            textures.len() + glyph_runs.iter().map(|run| run.range.len()).sum::<usize>();
        Self {
            textures: textures.into_iter().peekable(),
            glyph_runs: glyph_runs.into_iter(),
            active_glyph_run: None,
            remaining,
        }
    }
}

impl Iterator for UiOrderedQuadSlots {
    type Item = UiOrderedQuadSlot;

    fn next(&mut self) -> Option<Self::Item> {
        if self.active_glyph_run.is_none() {
            self.active_glyph_run = self.glyph_runs.next();
        }
        let texture_first = match (self.textures.peek(), &self.active_glyph_run) {
            (Some((texture, _)), Some(glyph)) => *texture <= glyph.next_order,
            (Some(_), None) => true,
            (None, _) => false,
        };
        let slot = if texture_first {
            self.textures
                .next()
                .map(|(_, index)| UiOrderedQuadSlot::Texture(index))
        } else if let Some(run) = &mut self.active_glyph_run {
            let index = run.range.next()?;
            run.next_order.3 += 1;
            if run.range.is_empty() {
                self.active_glyph_run = None;
            }
            Some(UiOrderedQuadSlot::Glyph(index))
        } else {
            None
        };
        if slot.is_some() {
            self.remaining -= 1;
        }
        slot
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for UiOrderedQuadSlots {}

#[cfg(test)]
#[path = "../../tests/render/ordered_quads.rs"]
mod ordered_quad_tests;
