//! Loading-screen phases and startup/world-transition progress orchestration.
//!
//! This boundary follows `LoadingScreen.cpp`. It reports real subsystem
//! progress and must not mask failed asset or network phases as completion.

mod loading_screen;

pub(crate) use loading_screen::{
    LoadingScreenDirectory, RuntimeLoadingScreen, RuntimeLoadingStage,
};
