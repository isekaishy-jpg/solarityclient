//! Build-12340 M2 parsing rules shared by model assets.

use crate::{AssetError, AssetPath};

/// The only M2 header version shipped by client build 12340.
const BUILD_12340_M2_VERSION: u32 = 264;

/// Applies build 12340's model-cache filename conversion before archive lookup.
///
/// Stock DBC records still carry legacy `.mdl` and `.mdx` names even though
/// the corresponding archive entry contains an M2. The cache accepts exactly
/// those two legacy extensions and `.m2`; it does not infer an absent or
/// unrelated extension.
pub(crate) fn canonical_model_path(path: &AssetPath) -> Result<AssetPath, AssetError> {
    let value = path.as_str();
    let Some((stem, extension)) = value.rsplit_once('.') else {
        return Err(model_decode(
            path,
            "model path has no supported extension".to_owned(),
        ));
    };

    match extension {
        "M2" => Ok(path.clone()),
        "MDL" | "MDX" => AssetPath::new(format!("{stem}.M2")),
        _ => Err(model_decode(
            path,
            format!("unsupported model path extension .{extension}"),
        )),
    }
}

/// Derives the stock `Model00.skin` through `ModelNN.skin` companion name.
pub(super) fn skin_path(model_path: &AssetPath, profile: u32) -> Result<AssetPath, AssetError> {
    let Some(stem) = model_path.as_str().strip_suffix(".M2") else {
        return Err(model_decode(
            model_path,
            "model path does not end in .m2".to_owned(),
        ));
    };

    AssetPath::new(format!("{stem}{profile:02}.skin"))
}

/// Rejects later MD21/chunked files and non-12340 legacy layouts up front.
pub(super) fn validate_model_prefix(path: &AssetPath, bytes: &[u8]) -> Result<(), AssetError> {
    let Some(prefix) = bytes.get(..8) else {
        return Err(model_decode(path, "M2 header is truncated".to_owned()));
    };
    if &prefix[..4] != b"MD20" {
        return Err(model_decode(
            path,
            "expected build-12340 MD20 magic".to_owned(),
        ));
    }

    let version = u32::from_le_bytes(
        prefix[4..8]
            .try_into()
            .map_err(|_| model_decode(path, "M2 version field is truncated".to_owned()))?,
    );
    if version != BUILD_12340_M2_VERSION {
        return Err(model_decode(
            path,
            format!("expected M2 version {BUILD_12340_M2_VERSION}, found {version}"),
        ));
    }
    Ok(())
}

/// Builds the stable asset error used for both M2 and companion failures.
pub(super) fn model_decode(path: &AssetPath, message: String) -> AssetError {
    AssetError::ModelDecode {
        path: path.clone(),
        message,
    }
}
