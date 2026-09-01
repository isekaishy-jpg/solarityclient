//! Main-thread pseudorandom streams retained by the stock client.

mod crt_rand;

pub use crt_rand::CrtRand;
pub use solarity_cpu::BlizzardRand;
