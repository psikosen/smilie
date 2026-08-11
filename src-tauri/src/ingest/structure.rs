//! Normalized paper representation (SRS §4.1).
//!
//! After OCR runs per page, we fold the block stream into this shape,
//! which is the input to the reasoning + generator models.

use serde::{Deserialize, Serialize};

use crate::runtime::OcrBlocks;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PaperJson {
    pub paper_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub sections: Vec<Section>,
    pub figures: Vec<FigureRef>,
    pub tables: Vec<TableRef>,
    pub equations: Vec<EquationRef>,
    pub references: Vec<String>,
    pub pages: Vec<PageText>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub id: String,
    pub title: String,
    pub level: u32,
    pub page_number: u32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FigureRef {
    pub id: String,
    pub page_number: u32,
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableRef {
    pub id: String,
    pub page_number: u32,
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquationRef {
    pub id: String,
    pub page_number: u32,
    pub latex: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageText {
    pub page_number: u32,
    pub text: String,
}

/// Fold a vec of per-page OCR results into a `PaperJson`. Best-effort
/// title inference picks the first heading block we see.
pub fn assemble(paper_id: &str, pages: &[OcrBlocks]) -> PaperJson {
    let mut paper = PaperJson {
        paper_id: paper_id.to_string(),
        ..Default::default()
    };

    let mut seen_title = false;
    let mut section_counter = 0u64;

    for (page_idx, page) in pages.iter().enumerate() {
        let page_number = (page_idx as u32) + 1;
        let mut page_text = String::new();

        for block in &page.blocks {
            let text = block.text.clone().unwrap_or_default();
            match block.block_type.as_str() {
                "heading" => {
                    if !seen_title {
                        paper.title = text.clone();
                        seen_title = true;
                    }
                    section_counter += 1;
                    paper.sections.push(Section {
                        id: format!("s{section_counter}"),
                        title: text.clone(),
                        level: 1,
                        page_number,
                        text: String::new(),
                    });
                }
                "figure" => paper.figures.push(FigureRef {
                    id: format!("f{}", paper.figures.len() + 1),
                    page_number,
                    caption: block.text.clone(),
                }),
                "table" => paper.tables.push(TableRef {
                    id: format!("t{}", paper.tables.len() + 1),
                    page_number,
                    caption: block.text.clone(),
                }),
                "equation" => paper.equations.push(EquationRef {
                    id: format!("e{}", paper.equations.len() + 1),
                    page_number,
                    latex: block.text.clone(),
                }),
                "reference" => paper.references.push(text.clone()),
                _ => {}
            }

            if !text.is_empty() {
                if let Some(section) = paper.sections.last_mut() {
                    if !section.text.is_empty() {
                        section.text.push_str("\n\n");
                    }
                    section.text.push_str(&text);
                }
                if !page_text.is_empty() {
                    page_text.push('\n');
                }
                page_text.push_str(&text);
            }
        }

        paper.pages.push(PageText {
            page_number,
            text: page_text,
        });
    }

    paper
}
