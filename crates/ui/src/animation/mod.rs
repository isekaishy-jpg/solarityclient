//! UI animation objects, groups, timelines, and scripted animation methods.
//!
//! `CSimpleAnim.cpp` and `CSimpleAnimScript.cpp` establish this stock family.
//! Animation state produces UI changes; it does not own the runtime event
//! scheduler or renderer frame clock.

mod c_simple_anim;
mod c_simple_anim_script;

pub use c_simple_anim::{
    UiAnimation, UiAnimationError, UiAnimationGroup, UiAnimationKind, UiAnimationLooping,
    UiAnimationPlan, UiAnimationValue,
};

pub(crate) use c_simple_anim_script::{
    UiAnimationMetatables, UiAnimationTransform, advance_animations, create_animation_metatables,
    owner_animation_transforms, register_owner_animations,
};
