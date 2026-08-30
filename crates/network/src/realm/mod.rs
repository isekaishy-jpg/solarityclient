//! Realm-list discovery and selected-realm transition evidenced by `RealmList.cpp` and `Login.cpp`.

mod login;
mod realm_list;

pub use realm_list::{RealmCategory, RealmDirectory, RealmEntry, RealmRecommendation, RealmType};
