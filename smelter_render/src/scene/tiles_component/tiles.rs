use crate::scene::{HorizontalAlign, Size, StatefulComponent, VerticalAlign};

use super::{interpolation::TileId, TilesComponentParams};

#[derive(Debug, Clone, Copy)]
struct RowsCols {
    rows: u32,
    columns: u32,
}

#[derive(Debug, Clone)]
pub(super) struct Tile {
    pub id: TileId,
    pub top: f32,
    pub left: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy)]
struct TilePosition {
    top: f32,
    left: f32,
    width: f32,
    height: f32,
}

impl TilesComponentParams {
    pub(super) fn tiles(&self, size: Size, children: &[StatefulComponent]) -> Vec<Option<Tile>> {
        let input_count = children.len() as u32;
        let rows_cols = self.optimal_row_column_count(input_count, size);
        let tile_size = self.tile_size(rows_cols, size);
        let tiles = self.tiles_positions(input_count, rows_cols, tile_size, size);
        let mut index = 0;
        tiles
            .into_iter()
            .zip(children.iter())
            .map(|(tile, child)| {
                Some(Tile {
                    top: tile.top,
                    left: tile.left,
                    width: tile.width,
                    height: tile.height,
                    id: match child.component_id() {
                        Some(id) => TileId::ComponentId(id.clone()),
                        None => {
                            let id = TileId::Index(index);
                            index += 1;
                            id
                        }
                    },
                })
            })
            .collect()
    }

    /// Optimize number of rows and cols to maximize space covered by tiles,
    /// preserving tile aspect_ratio
    fn optimal_row_column_count(&self, inputs_count: u32, layout_size: Size) -> RowsCols {
        fn from_rows_count(inputs_count: u32, rows: u32) -> RowsCols {
            let columns = inputs_count.div_ceil(rows);
            RowsCols { rows, columns }
        }
        let mut best_rows_cols = from_rows_count(inputs_count, 1);
        let mut best_tile_width = 0.0;

        for rows in 1..=inputs_count {
            let rows_cols = from_rows_count(inputs_count, rows);
            // larger width <=> larger tile size, because of const tile aspect ratio
            let tile_size = self.tile_size(rows_cols, layout_size).width;
            if tile_size > best_tile_width {
                best_rows_cols = rows_cols;
                best_tile_width = tile_size;
            }
        }

        best_rows_cols
    }

    fn tile_size(&self, rows_cols: RowsCols, layout_size: Size) -> Size {
        let x_padding = rows_cols.columns as f32 * 2.0 * self.padding;
        let y_padding = rows_cols.rows as f32 * 2.0 * self.padding;
        let x_margin = (rows_cols.columns as f32 + 1.0) * self.margin;
        let y_margin = (rows_cols.rows as f32 + 1.0) * self.margin;

        let x_scale = (layout_size.width - x_padding - x_margin).max(0.0)
            / rows_cols.columns as f32
            / self.tile_aspect_ratio.0 as f32;
        let y_scale = (layout_size.height - y_padding - y_margin).max(0.0)
            / rows_cols.rows as f32
            / self.tile_aspect_ratio.1 as f32;

        let scale = if x_scale < y_scale { x_scale } else { y_scale };

        Size {
            width: self.tile_aspect_ratio.0 as f32 * scale,
            height: self.tile_aspect_ratio.1 as f32 * scale,
        }
    }

    fn tiles_positions(
        &self,
        inputs_count: u32,
        rows_cols: RowsCols,
        tile_size: Size,
        layout_size: Size,
    ) -> Vec<TilePosition> {
        let mut layouts = Vec::with_capacity(inputs_count as usize);

        // Because scaled tiles with padding and margin don't have to cover whole output frame,
        // additional padding is distributed according to the alignment
        let additional_y_padding = layout_size.height
            - (tile_size.height + 2.0 * self.padding) * rows_cols.rows as f32
            - (self.margin * (rows_cols.rows as f32 + 1.0));

        let (additional_top_padding, justified_padding_y) = match self.vertical_align {
            VerticalAlign::Top => (0.0, 0.0),
            VerticalAlign::Center => (additional_y_padding / 2.0, 0.0),
            VerticalAlign::Bottom => (additional_y_padding, 0.0),
            VerticalAlign::Justified => {
                let space = additional_y_padding / (rows_cols.rows as f32 + 1.0);
                (0.0, space)
            }
        };

        let mut top = additional_top_padding + justified_padding_y + self.padding + self.margin;
        for row in 0..rows_cols.rows {
            let tiles_in_row = if row < rows_cols.rows - 1 {
                rows_cols.columns
            } else {
                inputs_count - ((rows_cols.rows - 1) * rows_cols.columns)
            };

            let additional_x_padding = layout_size.width
                - (tile_size.width + 2.0 * self.padding) * tiles_in_row as f32
                - (self.margin * (tiles_in_row as f32 + 1.0));

            let (additional_left_padding, justified_padding_x) = match self.horizontal_align {
                HorizontalAlign::Left => (0.0, 0.0),
                HorizontalAlign::Right => (additional_x_padding, 0.0),
                HorizontalAlign::Justified => {
                    let space = additional_x_padding / (tiles_in_row + 1) as f32;
                    (0.0, space)
                }
                HorizontalAlign::Center => (additional_x_padding / 2.0, 0.0),
            };

            let mut left =
                additional_left_padding + justified_padding_x + self.margin + self.padding;

            for _col in 0..tiles_in_row {
                layouts.push(TilePosition {
                    top,
                    left,
                    width: tile_size.width,
                    height: tile_size.height,
                });

                left += tile_size.width + self.margin + self.padding * 2.0 + justified_padding_x;
            }
            top += tile_size.height + self.margin + self.padding * 2.0 + justified_padding_y;
        }

        layouts
    }
}
