//! Manual fog is activated by screen-effect callbacks and retained between frames.

use solarity_asset::{WorldFogContext, WorldManualFog};
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct ScreenEffectFog {
    manual: Option<WorldManualFog>,
    kind: u32,
    pending: VecDeque<Change>,
    nether_reset: bool,
}

struct Change {
    kind: Option<u32>,
    context: Option<WorldFogContext>,
    full_screen_effects: bool,
}

impl ScreenEffectFog {
    pub(super) const fn special(&self) -> bool {
        self.kind == 3
    }
    pub(super) const fn nether(&self) -> bool {
        self.kind == 2
    }

    pub(super) fn take_nether_reset(&mut self) -> bool {
        std::mem::take(&mut self.nether_reset)
    }
    pub(super) const fn normal(&self) -> bool {
        self.kind == 0
    }

    pub(super) const fn ghost(&self) -> bool {
        self.kind == 1
    }
    pub(super) fn select(
        &mut self,
        kind: Option<u32>,
        context: Option<WorldFogContext>,
        full_screen_effects: bool,
    ) {
        self.pending.push_back(Change {
            kind,
            context,
            full_screen_effects,
        });
    }

    pub(super) fn resolve(&mut self, context: WorldFogContext) -> Option<WorldManualFog> {
        while let Some(change) = self.pending.pop_front() {
            // 8C02E0 invokes the outgoing owner's callback even when reselected.
            if self.kind == 2 && matches!(change.kind, None | Some(0..=3)) {
                self.nether_reset = true;
            }
            match change.kind {
                None => self.kind = 0,
                Some(kind @ 0..=3) => self.kind = kind,
                Some(_) => {}
            }
            match change.kind {
                Some(2) => {
                    let color = if change.full_screen_effects {
                        glam::Vec3::ONE
                    } else {
                        glam::Vec3::new(76., 76., 99.) / 255.
                    };
                    self.manual = Some(
                        change
                            .context
                            .unwrap_or(context)
                            .manual_fog(150., 0.7, color),
                    );
                }
                None | Some(0 | 1 | 3) => self.manual = None,
                // 4F7020's default branch preserves the previous fog/effect owner.
                Some(_) => {}
            }
        }
        self.manual
    }
}

#[cfg(test)]
#[path = "../../../tests/application/screen_effect_fog.rs"]
mod tests;
