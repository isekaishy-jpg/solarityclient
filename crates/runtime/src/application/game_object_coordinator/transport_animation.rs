//! Type-11 constructor sampling, receipt-time reversals, and 7139E0 frames.

use std::collections::VecDeque;

use glam::Vec3;
use solarity_asset::TransportCatalog;
use solarity_ecs::{ActiveWorld, GameObjectPresentation, WorldObjectIdentity};
use solarity_systems::{
    GameObjectPlacement, GameObjectPlacementResolver, TransportAnimationClock,
    TransportAnimationSample, TransportAnimationTrack,
};

use super::RuntimeGameObjectError;

/// Native behavior geometry and clocks survive asynchronous model admission.
pub(super) struct TransportAnimationState {
    track: TransportAnimationTrack,
    clock: TransportAnimationClock,
    position: Vec3,
    /// Behavior+0x34 consumes the first loaded-model observation only once.
    observed_ready_model: bool,
    /// Behavior+0x58 changes only while a dedicated map-model handle exists.
    sequence_id: u32,
    selected_sequence: Option<u32>,
    /// Long frames can select both a presample key and a current key, in order.
    pending_sequences: VecDeque<u32>,
    pub(super) passenger_time_ms: u32,
}

impl TransportAnimationState {
    /// 713820/711F20 sample immediately from the creation field image, before
    /// another raw block or a resource/template callback can replace that image.
    pub(super) fn new(
        world: &mut ActiveWorld,
        identity: WorldObjectIdentity,
        catalog: &TransportCatalog,
        fields: GameObjectPresentation,
        raw_time_ms: u32,
        resolver: &mut GameObjectPlacementResolver,
    ) -> Result<Self, RuntimeGameObjectError> {
        let guid = identity.guid();
        let entry = world
            .object_presentation(guid)
            .ok_or(RuntimeGameObjectError::MissingTransportCreation { guid })?
            .entry_id();
        let track =
            TransportAnimationTrack::new(catalog.animation(entry), catalog.rotation(entry))?;
        let mut state = Self {
            clock: TransportAnimationClock::new(
                track.period_ms(),
                fields.state() as i8,
                fields.transport_period_ms(),
                raw_time_ms,
                (fields.dynamic_word() >> 16) as u16,
            ),
            track,
            position: Vec3::ZERO,
            observed_ready_model: false,
            sequence_id: 0x1fa,
            selected_sequence: None,
            pending_sequences: VecDeque::with_capacity(2),
            passenger_time_ms: 0,
        };
        let base = resolver.resolve_animation_base(world, guid)?;
        let sample = state.sample(
            fields,
            raw_time_ms,
            base.rotation(),
            AnimationMapModel::Absent,
        )?;
        let parent = transport_parent_rotation(world, guid, resolver)?;
        let pose = sample.pose_with_transport_parent(base.matrix().w_axis.truncate(), parent)?;
        state.position = pose.matrix().w_axis.truncate();
        world.update_game_object_animated_pose(guid, pose)?;
        Ok(state)
    }

    pub(super) fn sequence_id(&self) -> Option<u32> {
        self.selected_sequence
    }

    /// 70B630 resets the key sentinel when recreating a dedicated model handle.
    pub(super) fn detach_model(&mut self) {
        self.sequence_id = 0x1fa;
        self.selected_sequence = None;
        self.pending_sequences.clear();
    }

    pub(super) fn take_sequence(&mut self) -> Option<u32> {
        self.pending_sequences.pop_front()
    }

    /// WMO handles have no CM2Model sequence timer to receive these requests.
    pub(super) fn discard_sequences(&mut self) {
        self.pending_sequences.clear();
    }

    /// The 711050 packet wrapper has already filtered identical state bytes.
    pub(super) fn notify_state(
        &mut self,
        fields: GameObjectPresentation,
        previous: u8,
        raw_time_ms: u32,
    ) {
        self.clock.notify_state(
            fields.transport_period_ms(),
            previous as i8,
            fields.state() as i8,
            raw_time_ms,
        );
    }

    /// Advances the native current-time sample using the global movement-frame
    /// delta, independent of this object's age or its resource-loading duration.
    pub(super) fn advance(
        &mut self,
        world: &mut ActiveWorld,
        guid: u64,
        fields: GameObjectPresentation,
        frame: AnimationFrame,
        resolver: &mut GameObjectPlacementResolver,
    ) -> Result<(), RuntimeGameObjectError> {
        if frame.map_model == AnimationMapModel::Ready && !self.observed_ready_model {
            self.observed_ready_model = true;
            if self.track.is_empty() {
                // 7139E0 writes the existing matrix and returns before publishing
                // either clock on the first ready-model frame with no DBC rows.
                return Ok(());
            }
        }
        let base = resolver.resolve_animation_base(world, guid)?;
        let parent = transport_parent_rotation(world, guid, resolver)?;
        let mut current_rotation = base.rotation();
        if frame.elapsed_ms > 250 {
            let start = self
                .clock
                .frame_start_ms(fields.transport_period_ms(), frame.raw_time_ms);
            let sample = self.sample(fields, start, current_rotation, frame.map_model)?;
            let pose =
                sample.pose_with_transport_parent(base.matrix().w_axis.truncate(), parent)?;
            self.position = pose.matrix().w_axis.truncate();
            // The presample stores packed rotation but leaves the public matrix
            // untouched. The next virtual quaternion getter sees that new word.
            current_rotation = GameObjectPlacement::from_animated_pose(pose, parent)?.rotation();
        }
        self.passenger_time_ms = self
            .clock
            .passenger_phase_ms(fields.transport_period_ms(), frame.raw_time_ms);
        let sample = self.sample(fields, frame.raw_time_ms, current_rotation, frame.map_model)?;
        let candidate = base.matrix().w_axis.truncate() + sample.offset;
        // 70C550 keeps subtraction and length extended; 7139E0 applies this
        // epsilon only with linked passengers. Ordinary GO model speed callbacks
        // and 760720 passenger collision pushes are separate outstanding owners.
        let [x, y, z] = std::array::from_fn::<_, 3, _>(|axis| {
            f64::from(candidate[axis]) - f64::from(self.position[axis])
        });
        let distance = ((z * z + y * y) + x * x).sqrt();
        if !frame.has_passengers || distance.abs() >= 1.0 / 1_048_576.0 {
            self.position = candidate;
        }
        let pose = TransportAnimationSample {
            offset: Vec3::ZERO,
            ..sample
        }
        .pose_with_transport_parent(self.position, parent)?;
        world.update_game_object_animated_pose(guid, pose)?;
        Ok(())
    }

    /// The key callback is conditional on a map handle, even during presampling.
    fn sample(
        &mut self,
        fields: GameObjectPresentation,
        raw_time_ms: u32,
        current_rotation: [f32; 4],
        map_model: AnimationMapModel,
    ) -> Result<TransportAnimationSample, RuntimeGameObjectError> {
        let phase = self.clock.sample_phase_ms(
            fields.transport_period_ms(),
            fields.state() as i8,
            raw_time_ms,
        );
        let sample = self
            .track
            .sample(phase, fields.parent_rotation(), current_rotation)?;
        if map_model == AnimationMapModel::Ready
            && let Some(sequence) = sample.sequence_id
            && self.sequence_id != sequence
        {
            self.sequence_id = sequence;
            self.selected_sequence = Some(sequence);
            self.pending_sequences.push_back(sequence);
        }
        Ok(sample)
    }
}

/// Inputs owned by the frame dispatcher and admitted passenger/model registries.
pub(super) struct AnimationFrame {
    pub raw_time_ms: u32,
    pub elapsed_ms: u32,
    pub map_model: AnimationMapModel,
    pub has_passengers: bool,
}

/// Resource and template admission jointly create the dedicated native handle.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum AnimationMapModel {
    Absent,
    Ready,
}

/// Resolve an actual movement parent; GAMEOBJECT_PARENTROTATION is unrelated.
fn transport_parent_rotation(
    world: &ActiveWorld,
    guid: u64,
    resolver: &mut GameObjectPlacementResolver,
) -> Result<Option<[f32; 4]>, RuntimeGameObjectError> {
    world
        .game_object_movement(guid)
        .and_then(|movement| movement.transport())
        .filter(|parent| parent.guid != 0)
        .map(|parent| {
            resolver
                .resolve(world, parent.guid)
                .map(|placement| placement.rotation())
        })
        .transpose()
        .map_err(Into::into)
}
