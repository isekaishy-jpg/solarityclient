//! Session-owned player UI snapshots and ordered server notifications.

use std::collections::VecDeque;

use super::environmental_damage::{RuntimeCombatLogClock, RuntimeEnvironmentalDamageSnapshot};
use solarity_network::WorldMirrorTimerUpdate;

/// Native replicated health, retained prediction and player ghost flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::application) struct RuntimePlayerHealthSnapshot {
    pub health: u32,
    pub maximum: u32,
    pub predicted: i32,
    pub ghost: bool,
}

impl RuntimePlayerHealthSnapshot {
    pub(in crate::application) fn from_world(world: &solarity_ecs::ActiveWorld) -> Option<Self> {
        let vitals = world.local_player_vitals()?;
        let predicted = world
            .unit_health_prediction(world.local_player_guid().ok()?)
            .map_or(
                vitals.health() as i32,
                solarity_ecs::UnitHealthPrediction::health,
            );
        let ghost = world
            .storage()
            .get::<&solarity_ecs::ObjectFields>(world.local_player())
            .is_ok_and(|fields| fields.get(150) & 0x10 != 0);
        Some(Self {
            health: vitals.health(),
            maximum: vitals.max_health(),
            predicted,
            ghost,
        })
    }

    pub(in crate::application) fn publish(self, world: &solarity_ui::UiWorldState) {
        if let Some(vitals) = world.player_vitals() {
            world.set_player_vitals(vitals.with_health(
                self.health,
                self.maximum,
                self.predicted,
                self.ghost,
            ));
        }
    }
}

/// One notification anchored to the same clock used by `GetMirrorTimerProgress`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::application) struct TimedMirrorTimerUpdate {
    pub update: WorldMirrorTimerUpdate,
    pub timestamp_ms: u32,
}

/// Shared ordering prevents a later flag packet changing an earlier timer trigger.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::application) enum RuntimePlayerUiNotification {
    MirrorTimer(TimedMirrorTimerUpdate),
    TutorialFlags(Vec<u8>),
    Combat(bool),
    EnvironmentalDamage(RuntimeEnvironmentalDamageSnapshot),
    Health {
        snapshot: RuntimePlayerHealthSnapshot,
        health_changed: bool,
        maximum_changed: bool,
    },
}

/// Retained independently of UI residency and cleared by native world exit.
#[derive(Default)]
pub(in crate::application) struct RuntimePlayerUiState {
    slots: [Option<TimedMirrorTimerUpdate>; 3],
    tutorial_flags: Vec<u8>,
    in_combat: bool,
    health: Option<RuntimePlayerHealthSnapshot>,
    combat_clock: RuntimeCombatLogClock,
    impacts: VecDeque<RuntimeEnvironmentalDamageSnapshot>,
    pending: VecDeque<RuntimePlayerUiNotification>,
}

impl RuntimePlayerUiState {
    pub(in crate::application) fn receive_environmental_damage(
        &mut self,
        world: &mut solarity_ecs::ActiveWorld,
        packet: solarity_network::WorldEnvironmentalDamage,
        name: Option<String>,
        factions: Option<&solarity_asset::CharacterFactionCatalog>,
        timestamp_ms: u32,
    ) {
        if let Some(snapshot) = RuntimeEnvironmentalDamageSnapshot::admit(
            world,
            packet,
            name,
            factions,
            timestamp_ms,
            self.combat_clock,
        ) {
            if snapshot.has_combat_event() {
                self.observe_health(world);
                self.pending
                    .push_back(RuntimePlayerUiNotification::EnvironmentalDamage(
                        snapshot.clone(),
                    ));
            }
            self.impacts.push_back(snapshot);
        }
    }

    pub(in crate::application) fn take_environmental_impact(
        &mut self,
    ) -> Option<RuntimeEnvironmentalDamageSnapshot> {
        self.impacts.pop_front()
    }

    #[cfg(test)]
    pub(in crate::application) fn with_combat_clock(
        mut self,
        clock: RuntimeCombatLogClock,
    ) -> Self {
        self.combat_clock = clock;
        self
    }

    pub(in crate::application) fn observe_health(&mut self, world: &solarity_ecs::ActiveWorld) {
        let Some(snapshot) = RuntimePlayerHealthSnapshot::from_world(world) else {
            return;
        };
        if self.health != Some(snapshot) {
            self.pending.push_back(RuntimePlayerUiNotification::Health {
                snapshot,
                health_changed: self.health.is_none_or(|old| old.health != snapshot.health),
                maximum_changed: self
                    .health
                    .is_none_or(|old| old.maximum != snapshot.maximum),
            });
            self.health = Some(snapshot);
        }
    }

    pub(in crate::application) fn observe_combat(&mut self, world: &solarity_ecs::ActiveWorld) {
        let in_combat = world
            .local_player_guid()
            .ok()
            .and_then(|guid| world.unit_flags(guid))
            .is_some_and(|flags| flags.primary() & 0x80000 != 0);
        if self.in_combat != in_combat {
            self.in_combat = in_combat;
            self.pending
                .push_back(RuntimePlayerUiNotification::Combat(in_combat));
        }
    }

    pub(in crate::application) fn receive_tutorial_flags(&mut self, flags: &[u8]) {
        self.tutorial_flags = flags.to_vec();
        self.pending
            .push_back(RuntimePlayerUiNotification::TutorialFlags(flags.to_vec()));
    }

    pub(in crate::application) fn tutorial_flags(&self) -> &[u8] {
        &self.tutorial_flags
    }
    pub(in crate::application) fn clear_for_world_leave(&mut self) {
        self.in_combat = false;
        self.health = None;
        self.impacts.clear();
        self.slots = [None; 3];
        self.pending.clear();
    }
    pub(in crate::application) fn receive(
        &mut self,
        update: WorldMirrorTimerUpdate,
        timestamp_ms: u32,
    ) {
        let notification = TimedMirrorTimerUpdate {
            update,
            timestamp_ms,
        };
        self.pending
            .push_back(RuntimePlayerUiNotification::MirrorTimer(notification));
        if let Some(slot) = self.slots.get_mut(update.timer() as usize) {
            match update {
                WorldMirrorTimerUpdate::Start { .. } => *slot = Some(notification),
                WorldMirrorTimerUpdate::Stop { .. } => *slot = None,
                // 519B03 emits an event without updating any BD0B80 field.
                WorldMirrorTimerUpdate::Pause { .. } => {}
            }
        }
    }

    pub(in crate::application) fn slots(&self) -> &[Option<TimedMirrorTimerUpdate>; 3] {
        &self.slots
    }

    pub(in crate::application) fn take_notification(
        &mut self,
    ) -> Option<RuntimePlayerUiNotification> {
        self.pending.pop_front()
    }

    pub(in crate::application) fn discard_published_notifications(&mut self) {
        self.pending.clear();
    }
}
