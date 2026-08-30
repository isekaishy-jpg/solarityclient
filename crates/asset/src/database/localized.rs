//! Exact physical locale-slot selection shared by build-12340 DBC schemas.

use crate::archive::{AssetError, Locale};

use super::wow_client_db::WdbcTable;

/// Maps accepted archive locale tokens to their physical locstring column.
///
/// English regional tokens share enUS-authored columns in the DBC schema; this
/// is a format alias and never substitutes for a missing localized value.
pub(super) const fn localized_string_index(locale: Locale) -> u32 {
    match locale {
        Locale::EnUs | Locale::EnGb | Locale::EnCn | Locale::EnTw => 0,
        Locale::KoKr => 1,
        Locale::FrFr => 2,
        Locale::DeDe => 3,
        Locale::ZhCn => 4,
        Locale::ZhTw => 5,
        Locale::EsEs => 6,
        Locale::EsMx => 7,
        Locale::RuRu => 8,
    }
}

/// Decodes one exact locale slot as client-authored UTF-8 text.
pub(super) fn localized_string(
    table: &WdbcTable,
    row: u32,
    first_field: u32,
    locale: Locale,
) -> Result<String, AssetError> {
    let field = first_field + localized_string_index(locale);
    let offset = table.field_u32(row, field).ok_or_else(|| {
        database_error(
            table,
            format!("record {row} locale field {field} is truncated"),
        )
    })?;
    let bytes = table.string_bytes(offset).ok_or_else(|| {
        database_error(
            table,
            format!("record {row} locale field {field} has invalid string offset {offset}"),
        )
    })?;
    String::from_utf8(bytes.to_vec()).map_err(|error| database_error(table, error.to_string()))
}

pub(super) fn database_error(table: &WdbcTable, message: String) -> AssetError {
    AssetError::DatabaseDecode {
        path: table.path().clone(),
        message,
    }
}
