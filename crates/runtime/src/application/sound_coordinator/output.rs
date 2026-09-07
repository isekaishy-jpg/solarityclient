//! Device-name reconciliation and explicit game sound-system restarts.

use solarity_media::{
    SoundEngineError, SoundOutput, SoundOutputConfiguration, SoundOutputDevice, SoundOutputQuality,
    SoundOutputTarget,
};

use super::{RuntimeSoundCoordinator, RuntimeSoundError, SoundCvarSource, software_channel_count};

#[cfg(test)]
#[path = "../../../tests/application/sound_output.rs"]
mod tests;

/// Retains the process target and the device catalog behind the current menu.
pub(super) struct RuntimeSoundOutput {
    target: SoundOutputTarget,
    default_name: String,
    devices: Option<Vec<SoundOutputDevice>>,
    selected_index: usize,
}

impl RuntimeSoundOutput {
    /// Resolves the saved name before opening the engine (native 8790C0).
    pub(super) fn resolve(
        cvars: &dyn SoundCvarSource,
        target: SoundOutputTarget,
        default_name: String,
    ) -> Result<Self, RuntimeSoundError> {
        let mut output = Self {
            target,
            default_name,
            devices: None,
            selected_index: 0,
        };
        output.refresh(cvars)?;
        Ok(output)
    }

    /// Re-enumerates physical outputs and resolves saved name/index together.
    fn refresh(&mut self, cvars: &dyn SoundCvarSource) -> Result<(), RuntimeSoundError> {
        if self.target != SoundOutputTarget::DefaultDevice {
            return Ok(());
        }
        let devices = SoundOutput::devices().map_err(SoundEngineError::from)?;
        let index = integer(cvars, "Sound_OutputDriverIndex")?;
        let name =
            cvars
                .sound_cvar("Sound_OutputDriverName")
                .ok_or(RuntimeSoundError::MissingCVar {
                    name: "Sound_OutputDriverName",
                })?;
        let names = std::iter::once(self.default_name.as_str())
            .chain(devices.iter().map(SoundOutputDevice::name))
            .collect::<Vec<_>>();
        self.selected_index = resolve_index(&names, index, &name);
        self.devices = Some(devices);
        Ok(())
    }

    /// Converts native 87C710's quality switch into the backend format request.
    pub(super) fn configuration(
        &self,
        cvars: &dyn SoundCvarSource,
    ) -> Result<SoundOutputConfiguration, RuntimeSoundError> {
        let quality = match integer(cvars, "Sound_OutputQuality")? {
            0 => SoundOutputQuality::Low,
            1 => SoundOutputQuality::Medium,
            _ => SoundOutputQuality::High,
        };
        let target = self
            .devices
            .as_ref()
            .and_then(|devices| {
                self.selected_index
                    .checked_sub(1)
                    .and_then(|index| devices.get(index))
            })
            .map_or(self.target, SoundOutputDevice::target);
        Ok(SoundOutputConfiguration { target, quality })
    }

    /// Updates profile state only after the selected output successfully opens.
    pub(super) fn publish(&self, cvars: &dyn SoundCvarSource) -> Result<(), RuntimeSoundError> {
        let Some(devices) = &self.devices else {
            return Ok(());
        };
        let name = self
            .selected_index
            .checked_sub(1)
            .and_then(|index| devices.get(index))
            .map_or(self.default_name.as_str(), SoundOutputDevice::name);
        cvars.publish_sound_output(
            devices
                .iter()
                .map(|device| device.name().to_owned())
                .collect(),
            self.selected_index,
            name,
        )?;
        Ok(())
    }
}

impl RuntimeSoundCoordinator {
    /// Supplies the current catalog before FrameXML's initial scripts run.
    pub(in crate::application) fn output_names(&self) -> Option<Vec<String>> {
        self.output.devices.as_ref().map(|devices| {
            devices
                .iter()
                .map(|device| device.name().to_owned())
                .collect()
        })
    }

    /// 985D30 stops playback while preserving the advanced/zone logical owners.
    /// SDL reopens synchronously, so it needs no FMOD-specific 200 ms delay.
    pub(super) fn restart_output(
        &mut self,
        cvars: &dyn SoundCvarSource,
    ) -> Result<(), RuntimeSoundError> {
        let replacement = RuntimeSoundOutput::resolve(
            cvars,
            self.output.target,
            self.output.default_name.clone(),
        )?;
        self.engine.restart(
            replacement.configuration(cvars)?,
            software_channel_count(cvars)?,
        )?;
        self.output = replacement;
        self.output.publish(cvars)?;
        Ok(())
    }
}

/// 8790C0 keeps a matching pair, searches by name, then selects index zero.
fn resolve_index(names: &[&str], index: i32, saved_name: &str) -> usize {
    if let Ok(index) = usize::try_from(index)
        && names
            .get(index)
            .is_some_and(|name| name.eq_ignore_ascii_case(saved_name))
    {
        return index;
    }
    names
        .iter()
        .position(|name| name.eq_ignore_ascii_case(saved_name))
        .unwrap_or(0)
}

/// Reads the registered signed output-option word without guessing a default.
fn integer(cvars: &dyn SoundCvarSource, name: &'static str) -> Result<i32, RuntimeSoundError> {
    let value = cvars
        .sound_cvar(name)
        .ok_or(RuntimeSoundError::MissingCVar { name })?;
    value
        .parse()
        .map_err(|_| RuntimeSoundError::InvalidOutputOption { name, value })
}
