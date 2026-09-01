//! Restricted HTML layout and presentation behavior evidenced by `CSimpleHTML.cpp`.

mod c_simple_html;

pub use c_simple_html::{
    UiSimpleHtmlAlignment, UiSimpleHtmlBlock, UiSimpleHtmlDocument, UiSimpleHtmlError,
    UiSimpleHtmlFontSlot, UiSimpleHtmlLine, UiSimpleHtmlNode, UiSimpleHtmlPlan,
};
