//! UI animation objects, groups, timelines, and scripted animation methods.
//!
//! `CSimpleAnim.cpp` and `CSimpleAnimScript.cpp` establish this stock family.
//! Animation state produces UI changes; it does not own the runtime event
//! scheduler or renderer frame clock.

mod c_simple_anim;
mod c_simple_anim_script;
