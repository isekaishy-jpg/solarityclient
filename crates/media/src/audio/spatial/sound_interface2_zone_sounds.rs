//! Native zone sound precedence, track completion, and authored silence.

use solarity_asset::{AreaSoundReferences, ZoneSoundCatalog};
use std::collections::BTreeMap;

/// Authored day/night column chosen by the realm's time-of-day owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZoneSoundTimeOfDay {
    /// First sound and delay column.
    Day,
    /// Second sound and delay column.
    Night,
}

impl ZoneSoundTimeOfDay {
    /// Chooses the native 05:30-inclusive, 21:00-exclusive daytime interval.
    /// `0x004c9850` uses realm time and the original `0x0076cf10` comparator.
    pub const fn from_day_milliseconds(milliseconds: u32) -> Self {
        let minute = (milliseconds % 86_400_000) / 60_000;
        if minute >= 330 && minute < 1260 {
            Self::Day
        } else {
            Self::Night
        }
    }
    /// Converts the two stock columns without consulting the machine clock.
    const fn index(self) -> usize {
        match self {
            Self::Day => 0,
            Self::Night => 1,
        }
    }
}

/// Location layers whose introductions occupy separate, higher priorities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZoneSoundLayer {
    /// Independently inherited AreaTable/WMOAreaTable relations.
    Area,
    /// A matching world-state override.
    WorldState,
    /// An authored terrain-chunk override.
    TerrainChunk,
}

impl ZoneSoundLayer {
    /// Normal layers 0..2 pair with introduction layers 5..7 in build 12340.
    const fn index(self) -> usize {
        match self {
            Self::Area => 0,
            Self::WorldState => 1,
            Self::TerrainChunk => 2,
        }
    }
}

/// Live gates affecting zone selection, independently of mixer volume.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ZoneSoundOptions {
    /// Sound_EnableAllSound plus the application's world playback gate.
    pub enabled: bool,
    /// Sound_EnableMusic; disabling it retires the current zone track.
    pub music: bool,
    /// Sound_EnableAmbience; disabling it retires the current ambient loop.
    pub ambience: bool,
    /// Sound_ZoneMusicNoDelay bypasses only normal zone-music silence.
    pub music_no_delay: bool,
}

/// Selected sound retaining the completion policy from its original layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ZoneMusicCue {
    sound_entry_id: u32,
    layer: usize,
    minimum_delay: u32,
    maximum_delay: u32,
}

impl ZoneMusicCue {
    /// Returns the exact SoundEntries key; zero explicitly selects silence.
    pub const fn sound_entry_id(self) -> u32 {
        self.sound_entry_id
    }
    /// Identifies an introduction that must finish before another intro starts.
    pub const fn is_intro(self) -> bool {
        matches!(self.layer, 5..=7)
    }
}

/// Desired music action; authored silence must not stop a still-playing track.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ZoneMusicSelection {
    /// No layer supplies music, or the category is disabled.
    Silence,
    /// Keep the current generation while the normal-zone delay is pending.
    Delay,
    /// Select this cue, retaining an existing generation with the same entry ID.
    Cue(ZoneMusicCue),
}

impl ZoneMusicSelection {
    /// Compares authored entry identity independently of layer and delay metadata.
    fn entry_id(self) -> Option<u32> {
        match self {
            Self::Cue(cue) => Some(cue.sound_entry_id),
            Self::Silence => Some(0),
            Self::Delay => None,
        }
    }
}

/// Nine-word native layer without the signed -1 absence sentinel.
#[derive(Clone, Copy, Debug, Default)]
struct Layer {
    music: [Option<u32>; 2],
    minimum_delay: [u32; 2],
    maximum_delay: [u32; 2],
    ambience: [Option<u32>; 2],
    provider: Option<u32>,
}

/// Process-owned zone selection and cooldowns, separate from device generations.
///
/// `0x004c86f0`, `0x004c88b0`, and the completion callbacks use eleven
/// descending layers. Missing DBC rows leave a layer absent; a present row's
/// zero sound is an explicit silent override, never a request for fallback.
#[derive(Default)]
pub struct ZoneSoundState {
    layers: [Layer; 11],
    next_music_ms: u32,
    intro_cooldowns: BTreeMap<u32, u32>,
    world_model_only: bool,
    underwater_liquid_id: u32,
}

/// Snapshot supplied to selection and callbacks on one application update.
#[derive(Clone, Copy, Debug)]
pub struct ZoneSoundFrame {
    /// Realm-authored day/night column.
    pub time: ZoneSoundTimeOfDay,
    /// Monotonic process milliseconds with the native 32-bit wrap.
    pub now_ms: u32,
    /// Generation still active after any retired generation was removed.
    pub active: Option<ZoneMusicCue>,
    /// Current category and scheduling gates.
    pub options: ZoneSoundOptions,
}

impl ZoneSoundState {
    /// Installs independently resolved relations when a location layer changes.
    /// Clock and option updates use selection without resetting these relations.
    pub fn set_location(
        &mut self,
        layer: ZoneSoundLayer,
        references: Option<AreaSoundReferences>,
        catalog: &ZoneSoundCatalog,
        frame: ZoneSoundFrame,
    ) {
        let index = layer.index();
        let supplied = references.is_some();
        let references = references.unwrap_or_default();
        let music = supplied
            .then(|| catalog.music(references.zone_music_id))
            .flatten();
        self.layers[index] = Layer {
            music: music.map_or([None; 2], |row| row.sound_entry_ids().map(Some)),
            minimum_delay: music.map_or([0; 2], |row| row.minimum_delay_ms()),
            maximum_delay: music.map_or([0; 2], |row| row.maximum_delay_ms()),
            ambience: supplied
                .then(|| catalog.ambience(references.ambience_id))
                .flatten()
                .map_or([None; 2], |row| row.sound_entry_ids().map(Some)),
            provider: supplied.then_some(references.sound_provider_id),
        };
        let intro = supplied
            .then(|| catalog.intro(references.intro_music_id))
            .flatten();
        self.layers[index + 5] = Layer {
            music: intro.map_or([None; 2], |row| [Some(row.sound_entry_id()); 2]),
            minimum_delay: intro.map_or([0; 2], |row| [row.minimum_delay_minutes(); 2]),
            ..Layer::default()
        };
        // 4C8D80 preserves pending normal-zone silence even across zone borders.
        // An invalid positive ZoneMusic key instead clears the deadline.
        if music.is_none() && references.zone_music_id > 0 {
            self.next_music_ms = 0;
        } else if music.is_some() {
            let selected = self.music(frame);
            if selected != ZoneMusicSelection::Delay
                && selected.entry_id() != frame.active.map(ZoneMusicCue::sound_entry_id)
            {
                self.next_music_ms = 0;
            }
        }
    }

    /// Installs the direct scripted SoundEntries override at native layer 3.
    pub fn set_script_music(&mut self, entry_id: Option<u32>) {
        self.layers[3] = Layer {
            music: [entry_id; 2],
            ..Layer::default()
        };
    }

    /// Installs the highest-priority authored music/ambience override.
    pub fn set_override(&mut self, music_id: u32, ambience_id: u32, catalog: &ZoneSoundCatalog) {
        let music = catalog.music(music_id);
        self.layers[10] = Layer {
            music: music.map_or([None; 2], |row| row.sound_entry_ids().map(Some)),
            minimum_delay: music.map_or([0; 2], |row| row.minimum_delay_ms()),
            maximum_delay: music.map_or([0; 2], |row| row.maximum_delay_ms()),
            ambience: catalog
                .ambience(ambience_id)
                .map_or([None; 2], |row| row.sound_entry_ids().map(Some)),
            provider: None,
        };
    }

    /// Updates the exterior suppression flag selected by the WMO location owner.
    pub fn set_world_model_only(&mut self, world_model_only: bool) {
        self.world_model_only = world_model_only;
    }

    /// Updates underwater ambience and provider independently of zone music.
    /// Returns whether the native immediate ambience replacement flag changed.
    pub fn set_underwater(&mut self, liquid_id: u32, provider_id: u32) -> bool {
        // 4C8630 compares the actual LiquidType key, including liquid-to-liquid
        // transitions. Zero is the scene's dry result (780620/790920).
        let changed = self.underwater_liquid_id != liquid_id;
        self.underwater_liquid_id = liquid_id;
        self.layers[9].ambience = [(liquid_id != 0).then_some(4209); 2];
        self.layers[9].provider = Some(provider_id);
        changed
    }

    /// Selects the highest available ambience column, including explicit silence.
    pub fn ambience(&self, time: ZoneSoundTimeOfDay, options: ZoneSoundOptions) -> Option<u32> {
        if !options.enabled || !options.ambience {
            return None;
        }
        self.layers
            .iter()
            .enumerate()
            .rev()
            .filter(|(index, _)| *index != 8 || !self.world_model_only)
            .find_map(|(_, layer)| layer.ambience[time.index()])
    }

    /// Selects the provider with the native underwater and WMO gates.
    pub fn sound_provider_id(&self) -> u32 {
        self.layers
            .iter()
            .enumerate()
            .rev()
            .filter(|(index, _)| {
                (*index != 8 || !self.world_model_only)
                    && (*index != 9 || self.underwater_liquid_id != 0)
            })
            .find_map(|(_, layer)| layer.provider)
            .unwrap_or(0)
    }

    /// Evaluates priority without consuming random words or altering playback.
    pub fn music(&self, frame: ZoneSoundFrame) -> ZoneMusicSelection {
        if !frame.options.enabled || !frame.options.music {
            return ZoneMusicSelection::Silence;
        }
        for (index, layer) in self.layers.iter().enumerate().rev() {
            if matches!(index, 5..=7)
                && let Some(cue) = frame.active.filter(|cue| cue.is_intro())
            {
                return ZoneMusicSelection::Cue(cue);
            }
            let Some(entry_id) = layer.music[frame.time.index()] else {
                continue;
            };
            if index <= 2 && !frame.options.music_no_delay && self.next_music_ms > frame.now_ms {
                return ZoneMusicSelection::Delay;
            }
            if matches!(index, 5..=7)
                && frame.active.map(ZoneMusicCue::sound_entry_id) != Some(entry_id)
                && self
                    .intro_cooldowns
                    .get(&entry_id)
                    .is_some_and(|deadline| *deadline > frame.now_ms)
            {
                continue;
            }
            return ZoneMusicSelection::Cue(ZoneMusicCue {
                sound_entry_id: entry_id,
                layer: index,
                minimum_delay: layer.minimum_delay[frame.time.index()],
                maximum_delay: layer.maximum_delay[frame.time.index()],
            });
        }
        ZoneMusicSelection::Silence
    }

    /// Runs a retired generation's callback, including interrupted fades.
    /// `frame.active` describes the generation still playing after retirement.
    /// The random boundary receives an exclusive upper bound, only when needed.
    pub fn finished(
        &mut self,
        cue: ZoneMusicCue,
        frame: ZoneSoundFrame,
        random_below: &mut impl FnMut(u32) -> u32,
    ) {
        match cue.layer {
            0..=2 => {
                if self.music(frame).entry_id() == Some(cue.sound_entry_id) {
                    let delay = if cue.minimum_delay < cue.maximum_delay {
                        cue.minimum_delay
                            .wrapping_add(random_below(cue.maximum_delay - cue.minimum_delay))
                    } else {
                        cue.maximum_delay
                    };
                    self.next_music_ms = frame.now_ms.wrapping_add(delay);
                } else {
                    self.next_music_ms = 0;
                }
            }
            3 => self.set_script_music(None),
            5..=7 => {
                self.intro_cooldowns.insert(
                    cue.sound_entry_id,
                    frame
                        .now_ms
                        .wrapping_add(cue.minimum_delay.wrapping_mul(60_000)),
                );
            }
            _ => {}
        }
    }
}

/// Resolves each relation independently, matching `0x0078e9a0`/`0x0078eeb0`.
/// The WMO-only gate removes both AreaTable inheritance sources.
pub fn resolve_zone_sound_references(
    zone: Option<AreaSoundReferences>,
    area: Option<AreaSoundReferences>,
    world_model: Option<AreaSoundReferences>,
    group: Option<AreaSoundReferences>,
    world_model_only: bool,
) -> AreaSoundReferences {
    let sources = [
        group,
        world_model,
        area.filter(|_| !world_model_only),
        zone.filter(|_| !world_model_only),
    ];
    let inherited = |field: fn(AreaSoundReferences) -> u32| {
        sources
            .iter()
            .flatten()
            .map(|source| field(*source))
            .find(|id| *id != 0)
            .unwrap_or(0)
    };
    AreaSoundReferences {
        sound_provider_id: inherited(|row| row.sound_provider_id),
        underwater_sound_provider_id: inherited(|row| row.underwater_sound_provider_id),
        ambience_id: inherited(|row| row.ambience_id),
        zone_music_id: inherited(|row| row.zone_music_id),
        intro_music_id: inherited(|row| row.intro_music_id),
    }
}
