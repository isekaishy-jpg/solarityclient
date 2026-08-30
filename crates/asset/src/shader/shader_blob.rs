//! Validated ownership of stock BLS permutation metadata and bytecode.

use crate::{ArchiveDescriptor, AssetError, AssetPath, AssetStore};

const BLS_MAGIC: &[u8; 4] = b"HSXG";
const BLS_VERSION_1_3: u32 = 0x0001_0003;
const VERTEX_SHADER_3_TOKEN: u32 = 0xFFFE_0300;
const PIXEL_SHADER_3_TOKEN: u32 = 0xFFFF_0300;
const HEADER_SIZE: usize = 12;
const BLOCK_HEADER_SIZE: usize = 16;

/// Shader Model 3 program stage carried by a build-12340 BLS library.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BlsShaderStage {
    /// Direct3D `vs_3_0` vertex permutations.
    Vertex,
    /// Direct3D `ps_3_0` pixel permutations.
    Pixel,
}

impl BlsShaderStage {
    /// Returns the exact leading Direct3D bytecode version token.
    const fn bytecode_token(self) -> u32 {
        match self {
            Self::Vertex => VERTEX_SHADER_3_TOKEN,
            Self::Pixel => PIXEL_SHADER_3_TOKEN,
        }
    }
}

/// One authored BLS permutation and the selectors stock uses to choose it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlsPermutation {
    flags_0: u32,
    flags_4: u32,
    selector: u32,
    bytecode: Vec<u8>,
}

impl BlsPermutation {
    /// Returns the first exact stock permutation mask word.
    #[must_use]
    pub const fn flags_0(&self) -> u32 {
        self.flags_0
    }

    /// Returns the second exact stock permutation mask word.
    #[must_use]
    pub const fn flags_4(&self) -> u32 {
        self.flags_4
    }

    /// Returns the exact third stock permutation selector word.
    #[must_use]
    pub const fn selector(&self) -> u32 {
        self.selector
    }

    /// Returns the complete Shader Model 3 token stream.
    #[must_use]
    pub fn bytecode(&self) -> &[u8] {
        &self.bytecode
    }
}

/// One archive-selected stock BLS library with every permutation retained.
#[derive(Clone, Debug)]
pub struct DecodedBlsShader {
    path: AssetPath,
    source: ArchiveDescriptor,
    stage: BlsShaderStage,
    permutations: Vec<BlsPermutation>,
}

impl DecodedBlsShader {
    /// Reads and validates one exact `vs_3_0` or `ps_3_0` BLS library.
    ///
    /// # Errors
    ///
    /// Returns archive errors or [`AssetError::ShaderDecode`] when the BLS
    /// wrapper, block ranges, alignment, or bytecode stage is malformed.
    pub fn load(
        store: &mut AssetStore,
        path: &AssetPath,
        stage: BlsShaderStage,
    ) -> Result<Self, AssetError> {
        let read = store.read(path)?;
        let source = read.source().clone();
        let permutations = decode_bls(path, read.bytes(), stage)?;
        Ok(Self {
            path: path.clone(),
            source,
            stage,
            permutations,
        })
    }

    /// Returns the normalized stock archive path.
    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    /// Returns the archive selected through ordinary patch precedence.
    #[must_use]
    pub const fn source(&self) -> &ArchiveDescriptor {
        &self.source
    }

    /// Returns the validated Shader Model 3 stage.
    #[must_use]
    pub const fn stage(&self) -> BlsShaderStage {
        self.stage
    }

    /// Returns every authored permutation in file order.
    #[must_use]
    pub fn permutations(&self) -> &[BlsPermutation] {
        &self.permutations
    }
}

/// Validates the BLS wrapper while preserving opaque permutation selectors.
fn decode_bls(
    path: &AssetPath,
    bytes: &[u8],
    stage: BlsShaderStage,
) -> Result<Vec<BlsPermutation>, AssetError> {
    let header = bytes
        .get(..HEADER_SIZE)
        .ok_or_else(|| shader_decode(path, "BLS header is truncated"))?;
    if &header[..4] != BLS_MAGIC {
        return Err(shader_decode(path, "BLS magic is not HSXG"));
    }
    let version = read_u32(header, 4);
    if version != BLS_VERSION_1_3 {
        return Err(shader_decode(
            path,
            format!("BLS version {version:#010x} is not build-12340 version 1.3"),
        ));
    }
    let count = read_u32(header, 8) as usize;
    if count == 0 {
        return Err(shader_decode(path, "BLS library has no permutations"));
    }

    let mut cursor = HEADER_SIZE;
    let mut permutations = Vec::with_capacity(count);
    for permutation_index in 0..count {
        let block_end = cursor
            .checked_add(BLOCK_HEADER_SIZE)
            .ok_or_else(|| shader_decode(path, "BLS block header range overflows"))?;
        let block = bytes.get(cursor..block_end).ok_or_else(|| {
            shader_decode(
                path,
                format!("BLS permutation {permutation_index} header is truncated"),
            )
        })?;
        let size = read_u32(block, 12) as usize;
        let bytecode_end = block_end
            .checked_add(size)
            .ok_or_else(|| shader_decode(path, "BLS bytecode range overflows"))?;
        let bytecode = bytes.get(block_end..bytecode_end).ok_or_else(|| {
            shader_decode(
                path,
                format!("BLS permutation {permutation_index} bytecode is truncated"),
            )
        })?;
        let token = bytecode.get(..4).ok_or_else(|| {
            shader_decode(
                path,
                format!("BLS permutation {permutation_index} has no shader token"),
            )
        })?;
        let actual_token = read_u32(token, 0);
        if actual_token != stage.bytecode_token() {
            return Err(shader_decode(
                path,
                format!(
                    "BLS permutation {permutation_index} has shader token {actual_token:#010x}, expected {:#010x}",
                    stage.bytecode_token()
                ),
            ));
        }
        permutations.push(BlsPermutation {
            flags_0: read_u32(block, 0),
            flags_4: read_u32(block, 4),
            selector: read_u32(block, 8),
            bytecode: bytecode.to_vec(),
        });
        cursor = bytecode_end
            .checked_add(3)
            .map(|value| value & !3)
            .ok_or_else(|| shader_decode(path, "BLS block alignment overflows"))?;
    }
    if cursor != bytes.len() {
        return Err(shader_decode(
            path,
            format!("BLS has {} trailing bytes", bytes.len().abs_diff(cursor)),
        ));
    }
    Ok(permutations)
}

/// Reads one already-bounds-checked little-endian word.
fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// Adds stable shader context without exposing a parser dependency.
fn shader_decode(path: &AssetPath, message: impl Into<String>) -> AssetError {
    AssetError::ShaderDecode {
        path: path.clone(),
        message: message.into(),
    }
}
