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
    ResurrectionOffer {
        offer: solarity_ui::UiPlayerResurrectionOffer,
        name: Option<String>,
    },
    CorpseRecovery(solarity_ui::UiPlayerCorpseState),
    CorpseLocation {
        corpse: solarity_ui::UiPlayerCorpseState,
        marker: [u32; 2],
        event: Option<&'static str>,
    },
    DeathAction(solarity_ui::UiPlayerDeathAction),
    PlayerAuras,
    Attack(bool),
    UnitDeath(super::unit_death::RuntimeUnitDeathSnapshot),
    Resurrection(RuntimePlayerResurrectionSnapshot),
    Life {
        snapshot: RuntimePlayerHealthSnapshot,
        event: RuntimePlayerLifeEvent,
        release_timer: solarity_ui::UiPlayerReleaseTimer,
    },
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

/// Replicated death-dialog inputs captured before their associated life event.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::application) struct RuntimePlayerResurrectionSnapshot {
    controlling: bool,
    flags: u32,
    spell: u32,
    blocked: bool,
    bypass_blocker: bool,
}

impl RuntimePlayerResurrectionSnapshot {
    pub(in crate::application) fn publish(
        self,
        world: &solarity_ui::UiWorldState,
        spells: &solarity_asset::SpellNameCatalog,
    ) {
        world.set_player_flags(self.flags);
        let mut state = world.resurrection_state();
        state.controlling = self.controlling;
        state.out_of_bounds = self.flags & 0x4000 != 0;
        state.self_resurrection_spell = self.spell;
        state.blocked = self.blocked;
        state.bypass_blocker = self.bypass_blocker;
        state.self_resurrection_name =
            (self.spell != 0).then(|| spells.name(self.spell).unwrap_or("UNKNOWN").to_owned());
        world.set_resurrection_state(state);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::application) enum RuntimePlayerLifeEvent {
    Dead,
    Alive,
    Flags { unghost: bool },
}

/// Retained independently of UI residency and cleared by native world exit.
#[derive(Default)]
pub(in crate::application) struct RuntimePlayerUiState {
    pub(super) names: super::player_names::RuntimePlayerNameCache,
    spells: Option<std::rc::Rc<solarity_asset::SpellEffectCatalog>>,
    slots: [Option<TimedMirrorTimerUpdate>; 3],
    tutorial_flags: Vec<u8>,
    in_combat: bool,
    health: Option<RuntimePlayerHealthSnapshot>,
    release_timer: solarity_ui::UiPlayerReleaseTimer,
    resurrection: RuntimePlayerResurrectionSnapshot,
    offer: solarity_ui::UiPlayerResurrectionOffer,
    pub(super) corpse: super::player_corpse::RuntimePlayerCorpse,
    pub(super) arena: bool,
    pub(super) screen_effect: super::screen_effect::RuntimePlayerScreenEffects,
    combat_clock: RuntimeCombatLogClock,
    impacts: VecDeque<RuntimeEnvironmentalDamageSnapshot>,
    pending: VecDeque<RuntimePlayerUiNotification>,
}

impl RuntimePlayerUiState {
    pub(in crate::application) fn receive_resurrection(
        &mut self,
        world: &solarity_ecs::ActiveWorld,
        update: solarity_network::WorldPlayerResurrection,
        now_ms: u32,
    ) {
        self.refresh_corpse_guid(world);
        match update {
            solarity_network::WorldPlayerResurrection::Offer {
                guid,
                name,
                sickness,
                timer,
            } => {
                self.offer = solarity_ui::UiPlayerResurrectionOffer {
                    guid,
                    sickness,
                    timer,
                };
                let display_name = if name.is_empty() {
                    RuntimePlayerHealthSnapshot::from_world(world)
                        .and_then(|_| self.names.request_for_offer(guid))
                } else {
                    Some(name)
                };
                let dead = RuntimePlayerHealthSnapshot::from_world(world)
                    .is_some_and(|v| v.health as i32 <= 0 || v.ghost);
                self.pending
                    .push_back(RuntimePlayerUiNotification::ResurrectionOffer {
                        offer: self.offer,
                        name: display_name.filter(|name| dead && !name.is_empty()),
                    });
            }
            solarity_network::WorldPlayerResurrection::RecoveryDelay(delay) => {
                self.corpse.ui.set_delay(delay, now_ms);
                self.pending
                    .push_back(RuntimePlayerUiNotification::CorpseRecovery(self.corpse.ui));
            }
        }
    }

    pub(in crate::application) fn offer(&self) -> solarity_ui::UiPlayerResurrectionOffer {
        self.offer
    }
    pub(in crate::application) fn corpse(&self) -> solarity_ui::UiPlayerCorpseState {
        self.corpse.ui
    }

    pub(in crate::application) fn corpse_marker(&self) -> [f32; 2] {
        self.corpse.marker
    }

    pub(super) fn corpse_world_entry(&mut self, world: &solarity_ecs::ActiveWorld) {
        self.screen_effect
            .refresh(world, self.spells.as_deref(), self.arena);
        let ghost = RuntimePlayerHealthSnapshot::from_world(world).is_some_and(|v| v.ghost);
        let event = self.corpse.clear(ghost, self.arena);
        self.publish_corpse(event);
    }

    pub(in crate::application) fn receive_corpse(
        &mut self,
        world: &solarity_ecs::ActiveWorld,
        update: solarity_network::WorldPlayerCorpseUpdate,
    ) {
        let event = self
            .corpse
            .receive(world, update, self.arena, super::player_corpse::seconds());
        self.corpse.ui.guid = world.local_corpse_guid();
        self.publish_corpse(event);
    }

    pub(in crate::application) fn advance_corpse(&mut self, world: &solarity_ecs::ActiveWorld) {
        let before = self.corpse.ui;
        let marker = self.corpse.marker.map(f32::to_bits);
        let event = self
            .corpse
            .advance(world, self.arena, super::player_corpse::seconds());
        if before != self.corpse.ui
            || marker != self.corpse.marker.map(f32::to_bits)
            || event.is_some()
        {
            self.publish_corpse(event);
        }
    }

    fn publish_corpse(&mut self, event: Option<&'static str>) {
        self.pending
            .push_back(RuntimePlayerUiNotification::CorpseLocation {
                corpse: self.corpse.ui,
                marker: self.corpse.marker.map(f32::to_bits),
                event,
            });
    }

    fn refresh_corpse_guid(&mut self, world: &solarity_ecs::ActiveWorld) {
        let guid = world.local_corpse_guid();
        if self.corpse.ui.guid != guid {
            self.corpse.ui.guid = guid;
            self.publish_corpse(None);
        }
    }

    pub(in crate::application) fn receive_player_name(
        &mut self,
        world: &solarity_ecs::ActiveWorld,
        response: solarity_network::WorldPlayerNameResponse,
    ) {
        for _ in 0..self.names.receive(response) {
            self.complete_offer_name(world);
        }
    }

    pub(super) fn complete_offer_name(&mut self, world: &solarity_ecs::ActiveWorld) {
        // 6DBC60 deliberately queries the CURRENT offer, regardless of which
        // request completed. A miss clears all offer globals, even while alive.
        let name = self.names.lookup(self.offer.guid).map(str::to_owned);
        if name.is_none() {
            self.offer = solarity_ui::UiPlayerResurrectionOffer::default();
        }
        let dead = RuntimePlayerHealthSnapshot::from_world(world)
            .is_some_and(|v| v.health as i32 <= 0 || v.ghost);
        self.pending
            .push_back(RuntimePlayerUiNotification::ResurrectionOffer {
                offer: self.offer,
                name: name.filter(|name| dead && !name.is_empty()),
            });
    }

    pub(in crate::application) fn synchronize_consumed_offer(
        &mut self,
        ui: solarity_ui::UiPlayerResurrectionOffer,
    ) {
        // Input callbacks consume the GUID even while the writer is full. Do
        // this before the next packet batch, keeping unpublished offers intact.
        if ui.guid == 0
            && self.offer.sickness == ui.sickness
            && self.offer.timer == ui.timer
            && !self
                .pending
                .iter()
                .any(|n| matches!(n, RuntimePlayerUiNotification::ResurrectionOffer { .. }))
        {
            self.offer.guid = 0;
        }
    }
    pub(super) fn set_spells(
        &mut self,
        spells: Option<std::rc::Rc<solarity_asset::SpellEffectCatalog>>,
    ) {
        self.spells = spells;
    }

    pub(super) fn receive_auras(
        &mut self,
        world: &mut solarity_ecs::ActiveWorld,
        update: solarity_network::WorldUnitAuraUpdate,
        receipt_ms: u32,
    ) {
        let local = world.local_player_guid().ok() == Some(update.guid);
        if local {
            self.screen_effect.before_auras(world, &update);
        }
        if super::unit_auras::apply(world, update, receipt_ms) && local {
            self.screen_effect
                .after_auras(world, self.spells.as_deref(), self.arena);
            self.refresh_resurrection(world);
            self.pending
                .push_back(RuntimePlayerUiNotification::PlayerAuras);
        }
    }
    pub(in crate::application) fn resurrection(&self) -> RuntimePlayerResurrectionSnapshot {
        self.resurrection
    }

    fn refresh_resurrection(&mut self, world: &solarity_ecs::ActiveWorld) {
        let Ok(fields) = world
            .storage()
            .get::<&solarity_ecs::ObjectFields>(world.local_player())
        else {
            return;
        };
        let snapshot = RuntimePlayerResurrectionSnapshot {
            controlling: (6..10).any(|index| fields.get(index) != 0),
            flags: fields.get(150),
            spell: fields.get(1199),
            blocked: self.spells.as_ref().is_some_and(|spells| {
                world
                    .storage()
                    .get::<&solarity_ecs::UnitAuras>(world.local_player())
                    .is_ok_and(|auras| solarity_systems::unit_has_aura_type(&auras, spells, 314))
            }),
            bypass_blocker: self
                .spells
                .as_ref()
                .and_then(|spells| spells.spell(fields.get(1199)))
                .is_some_and(|spell| spell.resurrection_bypass),
        };
        if snapshot != self.resurrection {
            self.resurrection = snapshot;
            self.pending
                .push_back(RuntimePlayerUiNotification::Resurrection(snapshot));
        }
    }
    pub(in crate::application) fn release_timer(&self) -> solarity_ui::UiPlayerReleaseTimer {
        self.release_timer
    }

    #[cfg(test)]
    pub(in crate::application) fn receive_unit_field(
        &mut self,
        world: &solarity_ecs::ActiveWorld,
        identity: solarity_ecs::WorldObjectIdentity,
        notification: crate::application::gameplay_session::UnitFieldNotification,
        timestamp_ms: u32,
    ) {
        self.receive_unit_field_with_sources(
            world,
            identity,
            notification,
            timestamp_ms,
            None,
            None,
        );
    }

    pub(super) fn receive_unit_field_with_sources(
        &mut self,
        world: &solarity_ecs::ActiveWorld,
        identity: solarity_ecs::WorldObjectIdentity,
        notification: crate::application::gameplay_session::UnitFieldNotification,
        timestamp_ms: u32,
        creatures: Option<&super::creature_cache::CreatureTemplateCache>,
        factions: Option<&solarity_asset::CharacterFactionCatalog>,
    ) {
        self.refresh_corpse_guid(world);
        let local = world.local_player_guid().ok() == Some(identity.guid());
        use crate::application::gameplay_session::UnitFieldNotification;
        // 6DA770 can refresh the local screen owner for any Player bytes callback.
        if let UnitFieldNotification::PlayerBytes2 { changed } = notification {
            self.screen_effect
                .vision_changed(world, self.spells.as_deref(), self.arena, changed);
        }
        if local {
            match notification {
                UnitFieldNotification::Initialize => {
                    self.screen_effect
                        .initialize(world, self.spells.as_deref(), self.arena)
                }
                UnitFieldNotification::PlayerFlags { previous } => {
                    let current = world
                        .storage()
                        .get::<&solarity_ecs::ObjectFields>(world.local_player())
                        .map_or(0, |f| f.get(150));
                    if (previous ^ current) & 0x10 != 0 {
                        self.screen_effect
                            .refresh(world, self.spells.as_deref(), self.arena);
                    }
                }
                _ => {}
            }
        }
        if matches!(
            notification,
            UnitFieldNotification::Initialize | UnitFieldNotification::PlayerBytes2 { .. }
        ) {
            return;
        }
        let entered_death = matches!(notification,
            crate::application::gameplay_session::UnitFieldNotification::Health { previous }
            if (previous as i32) > 0 && world.unit_vitals(identity.guid()).is_some_and(|vitals| (vitals.health() as i32) <= 0));
        if local {
            self.refresh_resurrection(world);
        }
        // 729220 calls 6DC0F0 before 7561E0 constructs the combat record.
        if local
            && entered_death
            && world.object_identity(identity.guid()) == Some(identity)
            && world
                .storage()
                .get::<&solarity_ecs::ObjectFields>(world.local_player())
                .is_ok_and(|fields| fields.get(79) & 0x20 == 0)
        {
            self.initialize_release_timer(world, timestamp_ms);
            // 6DC0F0 requests byte-zero release after timer setup, before the
            // death combat record. Admission belongs to this packet's aura image.
            if world
                .storage()
                .get::<&solarity_ecs::ObjectFields>(world.local_player())
                .is_ok_and(|fields| fields.get(59) & 0x100000 != 0)
                && (!self.resurrection.blocked || self.resurrection.bypass_blocker)
            {
                self.pending
                    .push_back(RuntimePlayerUiNotification::DeathAction(
                        solarity_ui::UiPlayerDeathAction::ReleaseSpirit { automatic: false },
                    ));
            }
        }
        if entered_death
            && let Some(mut death) = super::unit_death::RuntimeUnitDeathSnapshot::admit(
                world,
                identity,
                creatures,
                factions,
                timestamp_ms,
                self.combat_clock,
            )
        {
            death.player_ui = RuntimePlayerHealthSnapshot::from_world(world)
                .map(|health| (health, self.release_timer));
            self.pending
                .push_back(RuntimePlayerUiNotification::UnitDeath(death));
        }
        if !local {
            return;
        }
        let Some(snapshot) = RuntimePlayerHealthSnapshot::from_world(world) else {
            return;
        };
        match notification {
            UnitFieldNotification::Initialize | UnitFieldNotification::PlayerBytes2 { .. } => {}
            UnitFieldNotification::Health { previous } => {
                // Global 73F330 runs before the per-unit 60C240 UI observer.
                if (previous as i32) > 0 && (snapshot.health as i32) <= 0 {
                    self.life(snapshot, RuntimePlayerLifeEvent::Dead);
                } else if (previous as i32) <= 0 && (snapshot.health as i32) > 0 {
                    self.life(snapshot, RuntimePlayerLifeEvent::Alive);
                }
                self.pending.push_back(RuntimePlayerUiNotification::Health {
                    snapshot,
                    health_changed: true,
                    maximum_changed: false,
                });
                self.health = Some(snapshot);
            }
            UnitFieldNotification::MaximumHealth => {
                self.pending.push_back(RuntimePlayerUiNotification::Health {
                    snapshot,
                    health_changed: false,
                    maximum_changed: true,
                });
                self.health = Some(snapshot);
            }
            UnitFieldNotification::PlayerFlags { previous } => {
                self.life(
                    snapshot,
                    RuntimePlayerLifeEvent::Flags {
                        unghost: previous & 0x10 != 0 && !snapshot.ghost,
                    },
                );
                if (previous & 0x10 != 0) != snapshot.ghost {
                    // 6E0FD0 dispatches flags, then 6DF710 emits unghost (when
                    // needed) before clearing the location through 524A30.
                    let event = self.corpse.clear(snapshot.ghost, self.arena);
                    self.publish_corpse(event);
                }
            }
        }
    }

    fn life(&mut self, snapshot: RuntimePlayerHealthSnapshot, event: RuntimePlayerLifeEvent) {
        self.pending.push_back(RuntimePlayerUiNotification::Life {
            snapshot,
            event,
            release_timer: self.release_timer,
        });
    }

    fn initialize_release_timer(&mut self, world: &solarity_ecs::ActiveWorld, timestamp_ms: u32) {
        if let Ok(fields) = world
            .storage()
            .get::<&solarity_ecs::ObjectFields>(world.local_player())
        {
            self.release_timer = solarity_ui::UiPlayerReleaseTimer::on_death(
                fields.get(1197) as u8,
                fields.get(150),
                timestamp_ms,
            );
        }
    }

    /// Native 6E2E90's 37A notice refreshes the timer even without a field change.
    pub(in crate::application) fn receive_death_notice(
        &mut self,
        world: &solarity_ecs::ActiveWorld,
        timestamp_ms: u32,
    ) {
        self.refresh_resurrection(world);
        let Some(snapshot) = RuntimePlayerHealthSnapshot::from_world(world) else {
            return;
        };
        self.initialize_release_timer(world, timestamp_ms);
        self.life(
            snapshot,
            if (snapshot.health as i32) <= 0 {
                RuntimePlayerLifeEvent::Dead
            } else {
                RuntimePlayerLifeEvent::Alive
            },
        );
    }

    pub(in crate::application) fn receive_environmental_damage(
        &mut self,
        world: &mut solarity_ecs::ActiveWorld,
        packet: solarity_network::WorldEnvironmentalDamage,
        name: Option<String>,
        factions: Option<&solarity_asset::CharacterFactionCatalog>,
        template_flags: Option<u32>,
        timestamp_ms: u32,
    ) {
        if let Some(mut snapshot) = RuntimeEnvironmentalDamageSnapshot::admit(
            world,
            packet,
            name,
            factions,
            timestamp_ms,
            self.combat_clock,
        ) {
            snapshot.template_flags = template_flags;
            if snapshot.has_combat_event() {
                self.refresh_health(world);
                self.pending
                    .push_back(RuntimePlayerUiNotification::EnvironmentalDamage(
                        snapshot.clone(),
                    ));
            }
            self.impacts.push_back(snapshot);
        }
    }

    pub(in crate::application) fn receive_attack(
        &mut self,
        world: &mut solarity_ecs::ActiveWorld,
        attack: solarity_network::WorldUnitAttack,
    ) {
        if world.set_unit_attack_target(attack.attacker(), attack.retained_target())
            && world.local_player_guid().ok() == Some(attack.attacker())
        {
            self.pending
                .push_back(RuntimePlayerUiNotification::Attack(matches!(
                    attack,
                    solarity_network::WorldUnitAttack::Start { .. }
                )));
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

    /// Publish the polled image without inventing a native field callback.
    /// A later raw block may have overwritten the packet's watched old mirror.
    pub(in crate::application) fn refresh_health(&mut self, world: &solarity_ecs::ActiveWorld) {
        self.refresh_corpse_guid(world);
        self.refresh_resurrection(world);
        let Some(snapshot) = RuntimePlayerHealthSnapshot::from_world(world) else {
            return;
        };
        if self.health != Some(snapshot) {
            self.pending.push_back(RuntimePlayerUiNotification::Health {
                snapshot,
                health_changed: false,
                maximum_changed: false,
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
        self.offer = solarity_ui::UiPlayerResurrectionOffer::default();
        // Corpse position, recovery deadline and absent-transport clock belong
        // to retained UI globals. 524A30 clears the location on the next entry;
        // 52A980 resets recovery only when the whole FrameXML owner is created.
        self.resurrection = RuntimePlayerResurrectionSnapshot::default();
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
