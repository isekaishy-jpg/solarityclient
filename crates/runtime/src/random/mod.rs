//! Main-thread pseudorandom state retained from the linked stock C runtime.

mod crt_rand;

pub use crt_rand::CrtRand;
