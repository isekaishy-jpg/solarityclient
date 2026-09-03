//! External stock-compatibility tests for restricted `CSimpleHTML` documents.

use std::error::Error;

use solarity_asset::{ArchiveCatalog, AssetStore, ClientDataRoot, Locale, LocalizedDocument};
use solarity_ui::{
    GlueManager, UiSimpleHtmlAlignment, UiSimpleHtmlDocument, UiSimpleHtmlError,
    UiSimpleHtmlFontSlot,
};

use crate::support::{Fixture, FixtureFile};

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

/// The stock SimpleHTML method table accepts the empty dynamic document used
/// to clear native content without requiring a file-backed locale document.
#[test]
fn simple_html_set_text_is_available_to_frame_xml() -> Result<(), Box<dyn Error>> {
    let fixture = Fixture::new(&[
        FixtureFile {
            path: "Interface\\GlueXML\\GlueXML.toc",
            bytes: b"SimpleHtml.xml\n",
        },
        FixtureFile {
            path: "Interface\\GlueXML\\SimpleHtml.xml",
            bytes: br#"<Ui>
<Font name="HtmlFont" virtual="true"><FontHeight><AbsValue val="12"/></FontHeight></Font>
<SimpleHTML name="DynamicHtml" font="HtmlFont">
  <Size x="200" y="20"/><Anchors><Anchor point="CENTER"/></Anchors>
  <Scripts><OnLoad>self:SetText(""); HTML_SET_TEXT_CALLED = true</OnLoad></Scripts>
</SimpleHTML>
</Ui>"#,
        },
    ])?;
    let catalog =
        ArchiveCatalog::discover(ClientDataRoot::new(fixture.data_root())?, Locale::EnUs)?;
    let manager = GlueManager::start(AssetStore::mount(catalog)?, (1024, 768), false)?;

    assert!(
        manager
            .bundle()
            .lua()
            .globals()
            .get::<bool>("HTML_SET_TEXT_CALLED")?
    );
    Ok(())
}
