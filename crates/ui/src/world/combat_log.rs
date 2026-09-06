//! Retained combat-log presentation history and build-12340 filter semantics.
//!
//! Combat outcomes are supplied by the gameplay owner. This module preserves
//! their script arguments; it never synthesizes damage, healing, or spells.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use crate::{UiEventArgument, UiEventPayload};

/// Identity and classification flags already resolved for one combat participant.
#[derive(Clone, Debug, PartialEq)]
pub struct UiCombatLogObject {
    /// Native 64-bit world object identity, or zero for no object.
    pub guid: u64,
    /// Display name, including the realm suffix when supplied by the owner.
    pub name: Option<String>,
    /// Affiliation, reaction, control, type, and special classification bits.
    pub flags: u32,
}

/// Spell identity and localized display fields resolved from the client tables.
#[derive(Clone, Debug, PartialEq)]
pub struct UiCombatLogSpell {
    /// Spell table identifier.
    pub id: u32,
    /// Localized spell name, absent when the table row is unavailable.
    pub name: Option<String>,
    /// Native spell school mask.
    pub school: u32,
}

/// One resolved combat event, before Lua history filtering.
#[derive(Clone, Debug, PartialEq)]
pub struct UiCombatLogEntry {
    timestamp: f64,
    event_index: usize,
    source: UiCombatLogObject,
    destination: UiCombatLogObject,
    spell: Option<UiCombatLogSpell>,
    arguments: UiEventPayload,
}

impl UiCombatLogEntry {
    /// Owns a native combat event and its event-specific positional arguments.
    ///
    /// The arguments follow the common eight fields and optional spell triple.
    /// Explicit nil values and every trailing slot are preserved.
    ///
    /// # Errors
    /// Returns an error for an event outside build 12340's combat event table.
    pub fn new(
        timestamp: f64,
        event: &str,
        source: UiCombatLogObject,
        destination: UiCombatLogObject,
        spell: Option<UiCombatLogSpell>,
        arguments: UiEventPayload,
    ) -> Result<Self, UiCombatLogEventError> {
        let event_index = EVENT_NAMES
            .iter()
            .position(|name| *name == event)
            .ok_or_else(|| UiCombatLogEventError(event.to_owned()))?;
        Ok(Self {
            timestamp,
            event_index,
            source,
            destination,
            spell,
            arguments,
        })
    }

    /// Returns the native event name.
    #[must_use]
    pub fn event(&self) -> &'static str {
        EVENT_NAMES[self.event_index]
    }

    /// Builds the exact common header and already resolved event-specific tail.
    #[must_use]
    pub fn payload(&self) -> UiEventPayload {
        let mut arguments = vec![
            UiEventArgument::Number(self.timestamp),
            UiEventArgument::String(self.event().to_owned()),
        ];
        for object in [&self.source, &self.destination] {
            arguments.push(UiEventArgument::String(format!("0x{:016X}", object.guid)));
            arguments.push(
                object
                    .name
                    .as_ref()
                    .filter(|name| !name.is_empty())
                    .map_or(UiEventArgument::Nil, |name| {
                        UiEventArgument::String(name.clone())
                    }),
            );
            arguments.push(UiEventArgument::Integer(i64::from(object.flags)));
        }
        if let Some(spell) = self.spell.as_ref().filter(|spell| spell.id != 0) {
            arguments.push(UiEventArgument::Integer(i64::from(spell.id)));
            arguments.push(spell.name.as_ref().map_or(UiEventArgument::Nil, |name| {
                UiEventArgument::String(name.clone())
            }));
            arguments.push(UiEventArgument::Integer(i64::from(spell.school)));
        }
        arguments.extend_from_slice(self.arguments.arguments());
        UiEventPayload::new(arguments)
    }
}

/// An unsupported combat event was supplied by the gameplay projection.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
#[error("unknown build-12340 combat log event {0}")]
pub struct UiCombatLogEventError(String);

/// Shared main-thread combat history and current script filter/cursor.
#[derive(Clone, Debug, Default)]
pub struct UiCombatLogState {
    inner: Rc<RefCell<History>>,
}

#[derive(Debug, Default)]
struct History {
    entries: VecDeque<(u32, UiCombatLogEntry)>,
    filters: Vec<CombatLogFilter>,
    cursor: Option<usize>,
    active_text_unit: Option<u64>,
    spare_entries: usize,
}

impl UiCombatLogState {
    /// Creates empty history with no filters (all events accepted).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Retains an authoritative event at the process clock's current tick.
    /// Returns whether it matches the current script filters. Native admission
    /// (0x00750400) reuses one expired oldest entry when no cleared slot is free;
    /// elapsed time alone does not delete history.
    pub fn append(&self, entry: UiCombatLogEntry, now_ms: u32, retention_seconds: i32) -> bool {
        let mut history = self.inner.borrow_mut();
        if history.spare_entries != 0 {
            history.spare_entries -= 1;
        } else if history.entries.front().is_some_and(|(tick, _)| {
            now_ms
                .wrapping_sub((retention_seconds as u32).wrapping_mul(1000))
                .wrapping_sub(*tick) as i32
                >= 0
        }) {
            history.entries.pop_front();
            // Deleting the current native node advances to its next neighbor.
            history.cursor = history.cursor.and_then(|cursor| {
                if cursor > 0 {
                    Some(cursor - 1)
                } else {
                    (!history.entries.is_empty()).then_some(0)
                }
            });
        }
        let accepted = history.matches(&entry);
        history.entries.push_back((now_ms, entry));
        accepted
    }

    /// Removes all history and invalidates the current entry, preserving filters.
    pub fn clear(&self) {
        let mut history = self.inner.borrow_mut();
        history.spare_entries += history.entries.len();
        history.entries.clear();
        history.cursor = None;
    }

    /// Returns the GUID captured by CombatTextSetActiveUnit, if it resolved.
    #[must_use]
    pub fn active_text_unit(&self) -> Option<u64> {
        self.inner.borrow().active_text_unit
    }

    pub(crate) fn set_active_text_unit(&self, guid: Option<u64>) {
        self.inner.borrow_mut().active_text_unit = guid;
    }

    pub(crate) fn reset_filter(&self) {
        self.inner.borrow_mut().filters.clear();
    }

    pub(crate) fn add_filter(&self, filter: CombatLogFilter) {
        self.inner.borrow_mut().filters.push(filter);
    }

    pub(crate) fn count(&self, ignore_filter: bool) -> usize {
        let history = self.inner.borrow();
        history
            .entries
            .iter()
            .filter(|(_, entry)| ignore_filter || history.matches(entry))
            .count()
    }

    pub(crate) fn set_current(&self, index: i32, ignore_filter: bool) -> bool {
        let mut history = self.inner.borrow_mut();
        let mut entries = history
            .entries
            .iter()
            .enumerate()
            .filter(|(_, (_, entry))| ignore_filter || history.matches(entry));
        let selected = if index > 0 {
            entries.nth(index as usize - 1)
        } else {
            entries.rev().nth(index.unsigned_abs() as usize)
        }
        .map(|(index, _)| index);
        history.cursor = selected;
        selected.is_some()
    }

    pub(crate) fn advance(&self, count: i32, ignore_filter: bool) -> bool {
        let mut history = self.inner.borrow_mut();
        let Some(cursor) = history.cursor else {
            return false;
        };
        if count == 0 {
            return true;
        }
        // Native traversal counts matching entries starting at the current
        // node, including when a later filter change excludes that node.
        let entries = history
            .entries
            .iter()
            .enumerate()
            .filter(|(_, (_, entry))| ignore_filter || history.matches(entry));
        let selected = if count > 0 {
            entries
                .filter(|(index, _)| *index >= cursor)
                .nth(count as usize)
        } else {
            entries
                .rev()
                .filter(|(index, _)| *index <= cursor)
                .nth(count.unsigned_abs() as usize)
        }
        .map(|(index, _)| index);
        history.cursor = selected;
        selected.is_some()
    }

    pub(crate) fn current_payload(&self) -> Option<UiEventPayload> {
        let history = self.inner.borrow();
        history
            .entries
            .get(history.cursor?)
            .map(|(_, entry)| entry.payload())
    }
}

impl History {
    fn matches(&self, entry: &UiCombatLogEntry) -> bool {
        self.filters.is_empty() || self.filters.iter().any(|filter| filter.matches(entry))
    }
}

#[derive(Clone, Debug)]
pub(crate) enum CombatLogObjectFilter {
    Mask(u32),
    Guid(u64),
}

impl CombatLogObjectFilter {
    fn matches(&self, object: &UiCombatLogObject) -> bool {
        match self {
            Self::Guid(guid) => *guid != 0 && *guid == object.guid,
            Self::Mask(mask) => complete_object_mask(*mask & object.flags),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CombatLogFilter {
    pub(crate) events: u64,
    pub(crate) source: CombatLogObjectFilter,
    pub(crate) destination: CombatLogObjectFilter,
    pub(crate) spell_id: u32,
    pub(crate) spell_name: Option<String>,
}

impl CombatLogFilter {
    pub(crate) fn events(names: Option<&str>) -> u64 {
        names.map_or(u64::MAX, |names| {
            names
                .split([' ', ','])
                .filter_map(|token| {
                    EVENT_NAMES
                        .iter()
                        .position(|name| name.eq_ignore_ascii_case(token))
                })
                .fold(0, |mask, index| mask | (1_u64 << index))
        })
    }

    fn matches(&self, entry: &UiCombatLogEntry) -> bool {
        self.events & (1_u64 << entry.event_index) != 0
            && self.source.matches(&entry.source)
            && self.destination.matches(&entry.destination)
            && if self.spell_id != 0 {
                entry
                    .spell
                    .as_ref()
                    .is_some_and(|spell| spell.id == self.spell_id)
            } else if let Some(name) = self.spell_name.as_ref() {
                entry
                    .spell
                    .as_ref()
                    .and_then(|spell| spell.name.as_ref())
                    .is_some_and(|spell| native_folded_name(spell).eq(native_folded_name(name)))
            } else {
                true
            }
    }
}

/// Native 0x0074D1A0: any high special bit, or an intersection in every group.
pub(crate) const fn complete_object_mask(mask: u32) -> bool {
    mask & 0xffff_0000 != 0
        || (mask & 0xf != 0 && mask & 0xf0 != 0 && mask & 0x300 != 0 && mask & 0xfc00 != 0)
}

/// Native 0x0074D120 accepts a hex prefix and stops at the first non-hex byte.
pub(crate) fn parse_guid(text: &str) -> u64 {
    let text = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    text.bytes()
        .take(16)
        .map_while(|byte| (byte as char).to_digit(16))
        .fold(0, |guid, digit| (guid << 4) | u64::from(digit))
}

// 0x0076EA40 compares the alternate folded output from 0x0076E8D0.
// Stock's small mapping differs from Unicode case folding, notably for Russian.
fn native_folded_name(name: &str) -> impl Iterator<Item = u32> + '_ {
    name.chars().take_while(|ch| *ch != '\0').map(|ch| {
        let code = ch as u32;
        match code {
            0x61..=0x7a | 0xe0..=0xfe => code - 0x20,
            0x153 => 0x152,
            0x401 | 0x451 => 0x415,
            0x410..=0x415 => code - 1,
            0x430..=0x44f => code - u32::from(code < 0x436) - 0x20,
            0x10000.. => 0xfffd,
            _ => code,
        }
    })
}

const EVENT_NAMES: [&str; 50] = [
    "ENVIRONMENTAL_DAMAGE",
    "SWING_DAMAGE",
    "SWING_MISSED",
    "RANGE_DAMAGE",
    "RANGE_MISSED",
    "SPELL_CAST_START",
    "SPELL_CAST_SUCCESS",
    "SPELL_CAST_FAILED",
    "SPELL_MISSED",
    "SPELL_DAMAGE",
    "SPELL_HEAL",
    "SPELL_ENERGIZE",
    "SPELL_DRAIN",
    "SPELL_LEECH",
    "SPELL_INSTAKILL",
    "SPELL_SUMMON",
    "SPELL_CREATE",
    "SPELL_INTERRUPT",
    "SPELL_EXTRA_ATTACKS",
    "SPELL_DURABILITY_DAMAGE",
    "SPELL_DURABILITY_DAMAGE_ALL",
    "SPELL_AURA_APPLIED",
    "SPELL_AURA_APPLIED_DOSE",
    "SPELL_AURA_REMOVED_DOSE",
    "SPELL_AURA_REMOVED",
    "SPELL_AURA_REFRESH",
    "SPELL_DISPEL",
    "SPELL_STOLEN",
    "SPELL_AURA_BROKEN",
    "SPELL_AURA_BROKEN_SPELL",
    "DAMAGE_AURA_BROKEN",
    "ENCHANT_APPLIED",
    "ENCHANT_REMOVED",
    "SPELL_PERIODIC_MISSED",
    "SPELL_PERIODIC_DAMAGE",
    "SPELL_PERIODIC_HEAL",
    "SPELL_PERIODIC_ENERGIZE",
    "SPELL_PERIODIC_DRAIN",
    "SPELL_PERIODIC_LEECH",
    "SPELL_DISPEL_FAILED",
    "DAMAGE_SHIELD",
    "DAMAGE_SHIELD_MISSED",
    "DAMAGE_SPLIT",
    "PARTY_KILL",
    "UNIT_DIED",
    "UNIT_DESTROYED",
    "SPELL_RESURRECT",
    "SPELL_BUILDING_DAMAGE",
    "SPELL_BUILDING_HEAL",
    "UNIT_DISSIPATES",
];
