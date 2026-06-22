use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    widgets::{Block, Paragraph, Widget},
};

pub struct Map {
    rows: usize,
    cols: usize,
}

impl Map {
    pub fn default() -> Self {
        // Default to Monopoly board size (11x11)
        Self {
            rows: 11,
            cols: 11,
        }
    }
}

impl Widget for Map {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let col_constraints = (0..self.cols).map(|_| Constraint::Length(9));
        let row_constraints = (0..self.rows).map(|_| Constraint::Length(3));
        let horizontal = Layout::horizontal(col_constraints).spacing(1);
        let vertical = Layout::vertical(row_constraints).spacing(1);

        let cells = area.layout_vec(&vertical).into_iter().flat_map(|row| row.layout_vec(&horizontal));

        for (i, cell) in cells.enumerate() {
            Paragraph::new(format!("Area {:02}", i + 1))
                .block(Block::bordered())
                .render(cell, buf);
        }
    }
}