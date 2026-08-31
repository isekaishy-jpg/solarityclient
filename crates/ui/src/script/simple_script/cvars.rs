//! Script-visible console-variable state and stock startup registrations.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Debug, Eq, PartialEq)]
struct UiCVar {
    value: String,
    default: String,
    read_only: bool,
}

/// Shared CVar state queried and mutated by GlueXML script functions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct UiCVarRegistry {
    entries: Rc<RefCell<HashMap<String, UiCVar>>>,
}

impl UiCVarRegistry {
    /// Registers defaults recovered from the stock client startup path.
    pub(super) fn stock_initial() -> Self {
        let registry = Self::default();
        registry.register("accountName", "", false);
        registry.register("accountList", "", false);
        registry.register("showToolsUI", "-1", false);
        // These build-12340 defaults are consumed while GlueXML initializes
        // its video and audio option panels on SET_GLUE_SCREEN.
        for (name, default) in [
            ("useUiScale", "0"),
            ("uiScale", "1"),
            ("gxVSync", "1"),
            ("gxTripleBuffer", "0"),
            ("gxCursor", "1"),
            ("gxFixLag", "0"),
            ("gxWindow", "0"),
            ("gxMaximize", "0"),
            ("windowResizeLock", "0"),
            ("desktopGamma", "0"),
            ("gamma", "1"),
            ("gxResolution", "1920x1080"),
            ("gxRefresh", "60"),
            ("gxMultisample", "1"),
            ("farclip", "777"),
            ("shadowLevel", "0"),
            ("particleDensity", "1"),
            ("environmentDetail", "1"),
            ("groundEffectDensity", "64"),
            ("groundEffectDist", "140"),
            ("BaseMip", "0"),
            ("extShadowQuality", "2"),
            ("textureFilteringMode", "3"),
            ("weatherDensity", "3"),
            ("componentTextureLevel", "9"),
            ("specular", "1"),
            ("ffxGlow", "1"),
            ("ffxDeath", "1"),
            ("projectedTextures", "1"),
            ("gxStereoEnabled", "0"),
            ("gxStereoConvergence", "1"),
            ("gxStereoSeparation", "0"),
            ("Sound_EnableAllSound", "1"),
            ("Sound_EnableSFX", "1"),
            ("Sound_EnableMusic", "1"),
            ("Sound_EnableAmbience", "1"),
            ("Sound_EnableErrorSpeech", "1"),
            ("Sound_EnableEmoteSounds", "1"),
            ("Sound_EnablePetSounds", "1"),
            ("Sound_EnableDSPEffects", "1"),
            ("Sound_EnableReverb", "0"),
            ("Sound_EnableHardware", "0"),
            ("Sound_EnableSoftwareHRTF", "0"),
            ("Sound_EnableSoundWhenGameIsInBG", "0"),
            ("Sound_ListenerAtCharacter", "1"),
            ("Sound_MasterVolume", "1"),
            ("Sound_SFXVolume", "1"),
            ("Sound_MusicVolume", "0.4"),
            ("Sound_AmbienceVolume", "0.6"),
            ("Sound_NumChannels", "64"),
            ("Sound_MaxCacheSizeInBytes", "16777216"),
            ("Sound_MaxCacheableSizeInBytes", "1048576"),
            ("Sound_OutputQuality", "2"),
            ("Sound_OutputDriverIndex", "0"),
            ("Sound_ZoneMusicNoDelay", "0"),
        ] {
            registry.register(name, default, false);
        }
        registry
    }

    pub(super) fn get(&self, name: &str) -> Option<String> {
        self.entries
            .borrow()
            .get(&canonical_name(name))
            .map(|entry| entry.value.clone())
    }

    pub(super) fn default_value(&self, name: &str) -> Option<String> {
        self.entries
            .borrow()
            .get(&canonical_name(name))
            .map(|entry| entry.default.clone())
    }

    pub(super) fn minimum(&self, name: &str) -> Option<f64> {
        boundary(name, false)
    }

    pub(super) fn maximum(&self, name: &str) -> Option<f64> {
        boundary(name, true)
    }

    pub(super) fn boolean(&self, name: &str) -> bool {
        self.get(name)
            .and_then(|value| value.parse::<f64>().ok())
            .is_some_and(|value| value != 0.0)
    }

    pub(super) fn set(&self, name: &str, value: String) -> Result<(), UiCVarSetError> {
        let mut entries = self.entries.borrow_mut();
        let Some(entry) = entries.get_mut(&canonical_name(name)) else {
            return Err(UiCVarSetError::Missing);
        };
        if entry.read_only {
            return Err(UiCVarSetError::ReadOnly);
        }
        entry.value = value;
        Ok(())
    }

    fn register(&self, name: &str, default: &str, read_only: bool) {
        self.entries.borrow_mut().insert(
            canonical_name(name),
            UiCVar {
                value: default.to_owned(),
                default: default.to_owned(),
                read_only,
            },
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum UiCVarSetError {
    Missing,
    ReadOnly,
}

fn canonical_name(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn boundary(name: &str, maximum: bool) -> Option<f64> {
    let range = match canonical_name(name).as_str() {
        "uiscale" => (0.64, 1.0),
        "gamma" => (0.5, 1.5),
        "gxrefresh" => (0.0, 1_000.0),
        "gxmultisample" => (1.0, 8.0),
        "farclip" => (177.0, 1_277.0),
        "particledensity" => (0.1, 1.0),
        "environmentdetail" => (0.5, 1.5),
        "groundeffectdensity" => (16.0, 64.0),
        "groundeffectdist" => (70.0, 140.0),
        "extshadowquality" | "texturefilteringmode" => (0.0, 5.0),
        "weatherdensity" => (0.0, 3.0),
        "componenttexturelevel" => (8.0, 9.0),
        "sound_numchannels" => (32.0, 64.0),
        "sound_outputquality" => (0.0, 2.0),
        "useuiscale"
        | "gxvsync"
        | "gxtriplebuffer"
        | "gxcursor"
        | "gxfixlag"
        | "gxwindow"
        | "gxmaximize"
        | "windowresizelock"
        | "desktopgamma"
        | "shadowlevel"
        | "basemip"
        | "specular"
        | "ffxglow"
        | "ffxdeath"
        | "projectedtextures"
        | "gxstereoenabled"
        | "sound_enableallsound"
        | "sound_enablesfx"
        | "sound_enablemusic"
        | "sound_enableambience"
        | "sound_enableerrorspeech"
        | "sound_enableemotesounds"
        | "sound_enablepetsounds"
        | "sound_enabledspeffects"
        | "sound_enablereverb"
        | "sound_enablehardware"
        | "sound_enablesoftwarehrtf"
        | "sound_enablesoundwhengameisinbg"
        | "sound_listeneratcharacter"
        | "sound_mastervolume"
        | "sound_sfxvolume"
        | "sound_musicvolume"
        | "sound_ambiencevolume"
        | "sound_zonemusicnodelay" => (0.0, 1.0),
        "gxstereoconvergence" => (0.2, 50.0),
        "gxstereoseparation" => (0.0, 100.0),
        _ => return None,
    };
    Some(if maximum { range.1 } else { range.0 })
}
