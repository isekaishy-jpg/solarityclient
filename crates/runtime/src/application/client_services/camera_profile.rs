//! Character-owned ordinary camera persistence (518BF0/512890 and 5FF3E0/5FF320).

use solarity_ecs::PlayerViewState;

use super::ClientServices;
use crate::application::ApplicationError;
use crate::application::gameplay_coordinator::RuntimeGameplayError;

/// Registered defaults at 5FD910; vehicle state remains untouched by ordinary save.
const SAVED_CAMERA_DEFAULTS: [(&str, &str); 3] = [
    ("cameraSavedDistance", "5.55"),
    ("cameraSavedPitch", "10.0"),
    ("cameraSavedVehicleDistance", "-1.0"),
];

pub(super) fn is_saved_camera_cvar(name: &str) -> bool {
    SAVED_CAMERA_DEFAULTS
        .iter()
        .any(|(key, _)| key.eq_ignore_ascii_case(name))
}

impl ClientServices {
    /// The authenticated account, selected realm and entering character own this cache.
    pub(super) fn load_character_profile(
        &mut self,
        account: &str,
        character: &str,
    ) -> Result<(), ApplicationError> {
        let realm =
            self.selected_realm
                .as_ref()
                .ok_or_else(|| ApplicationError::NetworkRuntime {
                    message: "entered character has no selected realm".to_owned(),
                })?;
        self.character_profile = Some(self.startup_profile.character_profile(
            account,
            &realm.name,
            character,
        )?);
        self.player_movement.reset();
        Ok(())
    }

    /// Seed ECS before the movement owner copies its initial view, once per login.
    pub(super) fn restore_character_camera(&self) -> Result<(), ApplicationError> {
        if let (Some(profile), Some(world)) = (&self.character_profile, self.gameplay.world()) {
            let view = restore_view(
                profile.number("cameraSavedDistance", 5.55)?,
                profile.number("cameraSavedPitch", 10.)?,
            );
            world
                .set_local_player_view(view)
                .map_err(RuntimeGameplayError::from)?;
        }
        Ok(())
    }

    /// Stock 512890 saves current camera state before serializing character CVars.
    pub(super) fn save_character_camera(&mut self) -> Result<(), ApplicationError> {
        let Some(profile) = &mut self.character_profile else {
            return Ok(());
        };
        let view = match self.player_movement.camera_view() {
            Some(view) => view,
            None => match self.gameplay.world() {
                Some(world) => world
                    .local_player_view()
                    .map_err(RuntimeGameplayError::from)?,
                None => return Ok(()),
            },
        };
        profile.persist_cvars(&save_values(view))?;
        Ok(())
    }

    /// Load implemented character CVars after global settings and registered defaults.
    pub(super) fn world_cvar_values(&self) -> Vec<(String, String)> {
        let mut values = self
            .startup_profile
            .cvar_values()
            .iter()
            .filter(|(name, _)| !is_saved_camera_cvar(name))
            .cloned()
            .collect::<Vec<_>>();
        values
            .extend(SAVED_CAMERA_DEFAULTS.map(|(name, value)| (name.to_owned(), value.to_owned())));
        if let Some(profile) = &self.character_profile {
            values.extend(
                profile
                    .cvar_values()
                    .iter()
                    .filter(|(name, _)| is_saved_camera_cvar(name))
                    .cloned(),
            );
        }
        values
    }
}

/// 5FF3E0 clamps distance independently of the ordinary wheel maximum. Its x87
/// product uses the executable's f32 degree conversion constant before storage.
fn restore_view(distance: f32, pitch_degrees: f32) -> PlayerViewState {
    PlayerViewState::new(
        distance.clamp(0., 50.),
        (f64::from(pitch_degrees) * f64::from(f32::from_bits(0x3c8e_fa35))) as f32,
        0.,
        2,
    )
}

/// 5FF320 formats the live distance and pitch as doubles using ordinary `%f`.
/// Yaw is not among the saved ordinary-player fields.
fn save_numbers(view: PlayerViewState) -> [f64; 2] {
    [
        f64::from(view.distance()),
        f64::from(view.pitch_radians()) * f64::from(f32::from_bits(0x4265_2ee1)),
    ]
}

fn save_values(view: PlayerViewState) -> [(String, String); 2] {
    let [distance, pitch] = save_numbers(view);
    [
        ("cameraSavedDistance".to_owned(), format!("{distance:.6}")),
        ("cameraSavedPitch".to_owned(), format!("{pitch:.6}")),
    ]
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use glam::Vec3;
    use solarity_ecs::{ActiveWorld, PlayerViewState, WorldBootstrap, WorldMapId};

    use super::{restore_view, save_numbers, save_values};
    use crate::application::player_camera::{PlayerCameraInput, PlayerCameraZoomSettings};
    use crate::configuration::StartupProfile;

    /// Compare arithmetic at the original formatter/store boundaries, including clamps.
    #[test]
    fn camera_persistence_matches_original_instructions() -> Result<(), Box<dyn Error>> {
        let mut count = 0;
        for line in include_str!("../../../tests/fixtures/camera-persistence-native.txt")
            .lines()
            .filter(|line| !line.starts_with('#'))
        {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            let input = [
                f32::from_bits(u32::from_str_radix(fields[1], 16)?),
                f32::from_bits(u32::from_str_radix(fields[2], 16)?),
            ];
            if fields[0] == "restore" {
                let view = restore_view(input[0], input[1]);
                assert_eq!(
                    view.distance().to_bits(),
                    u32::from_str_radix(fields[3], 16)?,
                    "{line}"
                );
                assert_eq!(
                    view.pitch_radians().to_bits(),
                    u32::from_str_radix(fields[4], 16)?,
                    "{line}"
                );
            } else {
                assert_eq!(fields[0], "save");
                let values = save_numbers(PlayerViewState::new(input[0], input[1], 1., 2));
                assert_eq!(
                    values[0].to_bits(),
                    u64::from_str_radix(fields[3], 16)?,
                    "{line}"
                );
                assert_eq!(
                    values[1].to_bits(),
                    u64::from_str_radix(fields[4], 16)?,
                    "{line}"
                );
            }
            count += 1;
        }
        assert_eq!(count, 72);
        Ok(())
    }

    /// A fresh cache owner and ECS world recover the live obstructed camera after
    /// all previous in-memory owners are dropped, without sharing another character.
    #[test]
    fn character_camera_survives_process_lifecycle() -> Result<(), Box<dyn Error>> {
        let directory = std::env::temp_dir().join(format!(
            "solarity-camera-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let path = directory.join("WTF/Account/TEST/Realm/First/config-cache.wtf");
        fs::create_dir_all(path.parent().ok_or("missing test directory")?)?;
        fs::write(
            &path,
            "\u{feff}SET unrelated \"preserved\"\r\nSET cameraSavedDistance \"11\"\r\nSET cameraSavedPitch \"30\"\r\nSET cameraSavedVehicleDistance \"9\"\r\n",
        )?;
        let saved;
        {
            let startup = StartupProfile::load(&directory)?;
            let mut profile = startup.character_profile("TEST", "Realm", "First")?;
            let world = ActiveWorld::enter(WorldBootstrap::new(
                WorldMapId::new(1),
                1,
                "First",
                Vec3::ZERO,
                0.,
            ));
            world.set_local_player_view(restore_view(
                profile.number("cameraSavedDistance", 5.55)?,
                profile.number("cameraSavedPitch", 10.)?,
            ))?;
            let mut camera = PlayerCameraInput::new(world.local_player_view()?, 0.);
            camera.zoom(false, 3., 1000, PlayerCameraZoomSettings::default());
            camera.sample_zoom(1050, PlayerCameraZoomSettings::default());
            camera.obstructed(4.25, 1050);
            let view = camera.view(0.);
            // 6076C2 retains its documented obstruction bias in current distance.
            assert_eq!(view.distance(), 4.361_112);
            saved = save_values(view);
            profile.persist_cvars(&saved)?;
        }
        {
            let startup = StartupProfile::load(&directory)?;
            let first = startup.character_profile("TEST", "Realm", "First")?;
            let world = ActiveWorld::enter(WorldBootstrap::new(
                WorldMapId::new(1),
                1,
                "First",
                Vec3::ZERO,
                2.,
            ));
            world.set_local_player_view(restore_view(
                first.number("cameraSavedDistance", 5.55)?,
                first.number("cameraSavedPitch", 10.)?,
            ))?;
            let camera = PlayerCameraInput::new(world.local_player_view()?, 2.);
            assert_eq!(save_values(camera.view(2.)), saved);
            assert_eq!(camera.view(2.).yaw_offset_radians(), 0.);
            assert_eq!(first.number("cameraSavedVehicleDistance", -1.)?, 9.);
            let second = startup.character_profile("TEST", "Realm", "Second")?;
            assert_eq!(
                restore_view(
                    second.number("cameraSavedDistance", 5.55)?,
                    second.number("cameraSavedPitch", 10.)?
                ),
                PlayerViewState::default()
            );
            assert!(
                startup
                    .character_profile("TEST", "../escape", "First")
                    .is_err()
            );
            assert!(startup.cvar_values().is_empty());
        }
        let text = fs::read_to_string(&path)?;
        assert!(text.contains("SET unrelated \"preserved\""));
        assert_eq!(text.matches("SET cameraSavedDistance").count(), 1);
        fs::remove_file(path)?;
        Ok(())
    }
}
