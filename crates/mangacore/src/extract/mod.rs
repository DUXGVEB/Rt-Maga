//! Adaptive comic content extraction strategies (Native HTML and Structured Data / APIs).

pub mod html;
pub mod structured;

pub use html::{HtmlExtractionOutput, HtmlMinimumPolicy, extract_chapter_html_adaptive};
pub use structured::{
    StructuredChapterExtraction, extract_structured_chapter, extract_structured_manga,
    is_unsupported_api_payload,
};
