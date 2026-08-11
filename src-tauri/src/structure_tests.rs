//! Tests for the ingest::structure::assemble folding.

#[cfg(test)]
mod tests {
    use crate::ingest::structure::assemble;
    use crate::runtime::{OcrBlock, OcrBlocks};

    fn blocks(page: u32, items: &[(&str, &str)]) -> OcrBlocks {
        OcrBlocks {
            blocks: items
                .iter()
                .map(|(t, text)| OcrBlock {
                    page_number: page,
                    block_type: (*t).into(),
                    text: Some((*text).into()),
                    bbox: None,
                    confidence: None,
                })
                .collect(),
        }
    }

    #[test]
    fn first_heading_becomes_title() {
        let pages = vec![blocks(1, &[("heading", "Awesome Paper"), ("text", "body")])];
        let out = assemble("p-1", &pages);
        assert_eq!(out.title, "Awesome Paper");
    }

    #[test]
    fn accumulates_pages_and_sections() {
        let pages = vec![
            blocks(1, &[("heading", "Intro"), ("text", "Hello world.")]),
            blocks(2, &[("text", "More content.")]),
        ];
        let out = assemble("p-1", &pages);
        assert_eq!(out.pages.len(), 2);
        assert_eq!(out.sections.len(), 1);
        assert!(out.sections[0].text.contains("Hello world"));
        assert!(out.sections[0].text.contains("More content"));
    }

    #[test]
    fn categorises_figures_tables_equations() {
        let pages = vec![blocks(
            1,
            &[
                ("heading", "Title"),
                ("figure", "Fig. 1 caption"),
                ("table", "Tab. 1 caption"),
                ("equation", "E = mc^2"),
                ("reference", "Author, 2024"),
            ],
        )];
        let out = assemble("p-1", &pages);
        assert_eq!(out.figures.len(), 1);
        assert_eq!(out.tables.len(), 1);
        assert_eq!(out.equations.len(), 1);
        assert_eq!(out.references.len(), 1);
    }
}
