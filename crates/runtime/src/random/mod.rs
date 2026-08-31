//! Main-thread pseudorandom streams retained by the stock client.

mod blizzard_rand;
mod crt_rand;

pub use blizzard_rand::BlizzardRand;
pub use crt_rand::CrtRand;
