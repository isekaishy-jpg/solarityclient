//! Stock voice-chat capture, transport-facing frames, and playback state.
//!
//! This seed follows `SoundInterface2VoiceChat.cpp` and the stock ComSat source
//! family. It does not imply that a private-server deployment enables voice;
//! support must remain explicit rather than becoming an audio fallback path.

mod com_sat_client;
mod com_sat_sound_io_sound_engine;
mod sound_interface2_voice_chat;
