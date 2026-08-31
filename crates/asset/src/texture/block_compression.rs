//! Dependency-free descriptions of authored BLP block-compressed mip data.

/// Block compression carried directly by a BLP2 texture.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BlpBlockCompression {
    /// DXT1 color blocks, represented by Vulkan as BC1.
    Bc1,
    /// DXT3 explicit-alpha blocks, represented by Vulkan as BC2.
    Bc2,
    /// DXT5 interpolated-alpha blocks, represented by Vulkan as BC3.
    Bc3,
}

impl BlpBlockCompression {
    /// Returns the number of bytes in one 4x4 compression block.
    #[must_use]
    pub const fn block_byte_count(self) -> usize {
        match self {
            Self::Bc1 => 8,
            Self::Bc2 | Self::Bc3 => 16,
        }
    }

    /// Returns the tightly packed byte count for one mip extent.
    ///
    /// BLP dimensions are bounded to 16-bit values and the asset crate only
    /// compiles for 64-bit targets, so the block product is addressable.
    #[must_use]
    pub const fn mip_byte_count(self, width: u32, height: u32) -> usize {
        let block_width = width.div_ceil(4) as usize;
        let block_height = height.div_ceil(4) as usize;
        block_width * block_height * self.block_byte_count()
    }
}

/// Borrowed authored blocks and upload geometry for one BLP mip level.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlpBlockMip<'a> {
    compression: BlpBlockCompression,
    width: u32,
    height: u32,
    bytes: &'a [u8],
}

impl<'a> BlpBlockMip<'a> {
    pub(super) const fn new(
        compression: BlpBlockCompression,
        width: u32,
        height: u32,
        bytes: &'a [u8],
    ) -> Self {
        Self {
            compression,
            width,
            height,
            bytes,
        }
    }

    /// Returns the BC family corresponding to the authored DXT encoding.
    #[must_use]
    pub const fn compression(self) -> BlpBlockCompression {
        self.compression
    }

    /// Returns the mip's logical pixel width.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the mip's logical pixel height.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    /// Returns the whole authored blocks retained by the BLP parser.
    #[must_use]
    pub const fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    /// Returns the block-rounded byte count required by a GPU upload.
    #[must_use]
    pub const fn upload_byte_count(self) -> usize {
        self.compression.mip_byte_count(self.width, self.height)
    }
}
