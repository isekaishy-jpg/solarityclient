//! Exact Windows build-12340 proof accepted by strict AzerothCore realmd.

use sha1::{Digest, Sha1};

use super::{GruntIntegrity, LoginError};

/// AzerothCore's fixed version challenge for supported legacy clients.
const VERSION_CHALLENGE: [u8; 16] = [
    0xBA, 0xA3, 0x1E, 0x99, 0xA0, 0x0B, 0x21, 0x57, 0xFC, 0x37, 0x3F, 0xB3, 0x69, 0xCD, 0xD2, 0xF1,
];

/// `build_info.winChecksumSeed` for Windows build 12340.
const WINDOWS_CHECKSUM_SEED: [u8; 20] = [
    0xCD, 0xCB, 0xBD, 0x51, 0x88, 0x31, 0x5E, 0x6B, 0x4D, 0x19, 0x44, 0x9D, 0x49, 0x2D, 0xBC, 0xFA,
    0xF1, 0x56, 0xA3, 0x47,
];

/// Stateless strict-version proof for the pinned Windows/x86 wire identity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Build12340WindowsIntegrity;

impl GruntIntegrity for Build12340WindowsIntegrity {
    fn proof(
        &self,
        crc_salt: [u8; 16],
        client_public_key: [u8; 32],
    ) -> Result<[u8; 20], LoginError> {
        if crc_salt != VERSION_CHALLENGE {
            return Err(LoginError::Integrity {
                message: "server version challenge is not the build-12340 value".to_owned(),
            });
        }
        let mut digest = Sha1::new();
        digest.update(client_public_key);
        digest.update(WINDOWS_CHECKSUM_SEED);
        Ok(digest.finalize().into())
    }
}
