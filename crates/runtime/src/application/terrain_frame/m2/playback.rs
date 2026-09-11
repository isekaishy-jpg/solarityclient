//! A placement borrows gameplay playback or directly owns an ordinary model timer.

use std::cell::{Ref, RefCell, RefMut};
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

use crate::application::model_playback::M2Playback;

pub(super) enum M2PlaybackStorage {
    Local(M2Playback),
    Shared(Rc<RefCell<M2Playback>>),
}

impl M2PlaybackStorage {
    pub(super) fn into_shared(self) -> Rc<RefCell<M2Playback>> {
        match self {
            Self::Local(playback) => Rc::new(RefCell::new(playback)),
            Self::Shared(playback) => playback,
        }
    }

    pub(super) fn borrow(&self) -> M2PlaybackRead<'_> {
        match self {
            Self::Local(playback) => M2PlaybackRead::Local(playback),
            Self::Shared(playback) => M2PlaybackRead::Shared(playback.borrow()),
        }
    }

    pub(super) fn borrow_mut(&mut self) -> M2PlaybackWrite<'_> {
        match self {
            Self::Local(playback) => M2PlaybackWrite::Local(playback),
            Self::Shared(playback) => M2PlaybackWrite::Shared(playback.borrow_mut()),
        }
    }

    pub(super) fn into_local(self) -> Option<M2Playback> {
        match self {
            Self::Local(playback) => Some(playback),
            Self::Shared(_) => None,
        }
    }
}

pub(super) enum M2PlaybackRead<'a> {
    Local(&'a M2Playback),
    Shared(Ref<'a, M2Playback>),
}

impl Deref for M2PlaybackRead<'_> {
    type Target = M2Playback;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Local(playback) => playback,
            Self::Shared(playback) => playback,
        }
    }
}

pub(super) enum M2PlaybackWrite<'a> {
    Local(&'a mut M2Playback),
    Shared(RefMut<'a, M2Playback>),
}

impl Deref for M2PlaybackWrite<'_> {
    type Target = M2Playback;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Local(playback) => playback,
            Self::Shared(playback) => playback,
        }
    }
}

impl DerefMut for M2PlaybackWrite<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Local(playback) => playback,
            Self::Shared(playback) => playback,
        }
    }
}
