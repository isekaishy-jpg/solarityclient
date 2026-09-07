//! Realm-clock zone selection and asynchronous playback at the composition root.

use super::{
    RuntimeGlueVoiceIdentity, RuntimeSoundCoordinator, RuntimeSoundError, SoundCvarSource, boolean,
};
use crate::random::BlizzardRand;
use crate::time::RealmClock;
use solarity_media::{ZoneSoundFrame, ZoneSoundLayer, ZoneSoundOptions, ZoneSoundTimeOfDay};

impl RuntimeSoundCoordinator {
    /// Stages independently inherited relations from the authoritative location.
    pub(in crate::application) fn stage_zone(
        &mut self,
        location: super::super::character_directory::RuntimeWorldLocation,
        world: Option<&solarity_ecs::ActiveWorld>,
        underwater_liquid_id: u32,
    ) {
        self.next_zone_references = Some(location.sounds);
        self.next_chunk_references = location
            .chunk_key
            .and_then(|key| self.zone_overrides.chunk(key));
        self.next_state_references = world.and_then(|world| {
            solarity_media::resolve_world_state_zone_sounds(
                self.zone_overrides.world_states(),
                location.sound_location_ids,
                |field| world.world_state_values().value(field),
            )
        });
        self.zone.set_environment(
            location.world_model_only,
            underwater_liquid_id,
            location.sounds.underwater_sound_provider_id,
        );
    }

    /// Applies selection only after live settings and the world listener are current.
    pub(super) fn update_zone(
        &mut self,
        cvars: &dyn SoundCvarSource,
        clock: &RealmClock,
        random: &mut BlizzardRand,
    ) -> Result<(), RuntimeSoundError> {
        let frame = ZoneSoundFrame {
            time: ZoneSoundTimeOfDay::from_day_milliseconds(clock.day_milliseconds()),
            now_ms: self.started.elapsed().as_millis() as u32,
            active: self.zone.active_music(),
            options: ZoneSoundOptions {
                enabled: boolean(cvars, "Sound_EnableAllSound")?,
                music: boolean(cvars, "Sound_EnableMusic")?,
                ambience: boolean(cvars, "Sound_EnableAmbience")?,
                music_no_delay: boolean(cvars, "Sound_ZoneMusicNoDelay")?,
            },
        };
        if self.zone_references != self.next_zone_references {
            self.zone_references = self.next_zone_references;
            self.zone
                .set_location(ZoneSoundLayer::Area, self.zone_references, frame);
        }
        let script_music = self
            .glue_music
            .as_ref()
            .is_some_and(|voice| matches!(voice.identity, RuntimeGlueVoiceIdentity::File(_)));
        if self.chunk_references != self.next_chunk_references {
            self.chunk_references = self.next_chunk_references;
            self.zone
                .set_location(ZoneSoundLayer::TerrainChunk, self.chunk_references, frame);
        }
        if self.state_references != self.next_state_references {
            self.state_references = self.next_state_references;
            self.zone
                .set_location(ZoneSoundLayer::WorldState, self.state_references, frame);
        }
        let loads = self.engine.with_engine_mut(|engine| {
            self.zone
                .update(engine, frame, script_music, &mut || random.next_u32())
        })?;
        for load in loads {
            self.loader.queue(load);
        }
        Ok(())
    }
}
