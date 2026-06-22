use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    widgets::{Block, Widget},
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
        let col_constraints = (0..self.cols).map(|_| Constraint::Length(8));
        let row_constraints = (0..self.rows).map(|_| Constraint::Length(4));
        let horizontal = Layout::horizontal(col_constraints);
        let vertical = Layout::vertical(row_constraints);

        let mut n = 0;
        for (r, row) in area.layout_vec(&vertical).into_iter().enumerate() {
            for (c, cell) in row.layout_vec(&horizontal).into_iter().enumerate() {
                let is_ring = r == 0 || r == self.rows - 1 || c == 0 || c == self.cols - 1;
                if !is_ring {
                    continue;
                }
                n += 1;
                Block::bordered()
                    .title(format!("{:02}", n))
                    .render(cell, buf);
            }
        }
    }
}