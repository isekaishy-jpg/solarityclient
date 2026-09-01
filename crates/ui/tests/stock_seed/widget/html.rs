//! External stock-compatibility tests for restricted `CSimpleHTML` documents.

use std::error::Error;

use solarity_asset::LocalizedDocument;
use solarity_ui::{
    UiSimpleHtmlAlignment, UiSimpleHtmlDocument, UiSimpleHtmlError, UiSimpleHtmlFontSlot,
};

/// Locale HTML preserves only the stock block vocabulary and explicit breaks.
#[test]
fn localized_document_parses_stock_blocks() -> Result<(), Box<dyn Error>> {
    let source = b"\xef\xbb\xbf<html><body>\n\
<p align='right'> alpha\n beta<br/> gamma </p><br/>\
<h2 align='Center'>Title &#33; &#8220;x&#8221;</h2>\
</body></html>";

    let document = UiSimpleHtmlDocument::parse(LocalizedDocument::Eula, source)?;
    let blocks = document.blocks();

    assert_eq!(document.source(), LocalizedDocument::Eula);
    assert_eq!(blocks.len(), 3);
    assert_eq!(blocks[0].font(), UiSimpleHtmlFontSlot::Normal);
    assert_eq!(blocks[0].alignment(), UiSimpleHtmlAlignment::Right);
    assert_eq!(blocks[0].text(), "alpha beta|ngamma");
    assert_eq!(blocks[1].text(), "|n");
    assert_eq!(blocks[2].font(), UiSimpleHtmlFontSlot::Header2);
    assert_eq!(blocks[2].alignment(), UiSimpleHtmlAlignment::Center);
    assert_eq!(blocks[2].text(), "Title ! “x”");
    Ok(())
}

/// A locale file outside the recovered HTML/BODY shape is not treated as text.
#[test]
fn localized_document_rejects_non_stock_root() {
    let result = UiSimpleHtmlDocument::parse(LocalizedDocument::Tos, b"<document/>");

    assert!(matches!(result, Err(UiSimpleHtmlError::Load(_))));
}
