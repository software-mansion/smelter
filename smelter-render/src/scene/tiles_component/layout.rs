use crate::{
    scene::{
        BorderRadius, RGBAColor, Size,
        layout::{ChildComponent, ChildPlacement, LayoutChild},
    },
    transformations::layout::{LayoutContent, NestedLayout},
};

use super::tiles::Tile;

/// Places `children` in `tiles` and returns a layout of the tiles component itself. Children
/// without a tile are not shown.
pub(super) fn place_tiles(
    tiles: &[Option<Tile>],
    size: Size,
    children: &mut [LayoutChild],
    background_color: RGBAColor,
) -> NestedLayout {
    for (child, tile) in children.iter_mut().zip(tiles) {
        match tile {
            Some(tile) => child.place(tile_placement(tile, child)),
            None => child.hide(),
        }
    }

    NestedLayout {
        top: 0.0,
        left: 0.0,
        width: size.width,
        height: size.height,
        rotation_degrees: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        crop: None,
        mask: None,
        content: LayoutContent::Color(background_color),
        children: vec![],
        border_width: 0.0,
        border_color: RGBAColor(0, 0, 0, 0),
        border_radius: BorderRadius::ZERO,
        box_shadow: vec![],
    }
}

fn tile_placement(tile: &Tile, child: &LayoutChild) -> ChildPlacement {
    let tile_size = Size {
        width: tile.width,
        height: tile.height,
    };
    match &child.component {
        ChildComponent::Layout(_) => ChildPlacement::new(tile.top, tile.left, tile_size),
        ChildComponent::Node(node) => fit_into_tile(tile, node.size()),
    }
}

/// The largest area in the middle of the `tile` that has the same aspect ratio as `size`.
fn fit_into_tile(tile: &Tile, size: Size) -> ChildPlacement {
    let scale_to_fit_width = tile.width / size.width;
    let scale_to_fit_height = tile.height / size.height;
    let scale_factor = f32::min(scale_to_fit_width, scale_to_fit_height);

    let top_offset = (tile.height - scale_factor * size.height) / 2.0;
    let left_offset = (tile.width - scale_factor * size.width) / 2.0;

    ChildPlacement::new(
        tile.top + top_offset,
        tile.left + left_offset,
        Size {
            width: scale_factor * size.width,
            height: scale_factor * size.height,
        },
    )
}

pub(super) fn resize_tiles(
    tiles: &[Option<Tile>],
    original_size: &Size,
    desired_size: &Size,
) -> Vec<Option<Tile>> {
    let scale = f32::min(
        desired_size.width / original_size.width,
        desired_size.height / original_size.height,
    );
    tiles
        .iter()
        .map(|tile| {
            tile.as_ref().map(|tile| Tile {
                id: tile.id.clone(),
                top: tile.top * scale,
                left: tile.left * scale,
                width: tile.width * scale,
                height: tile.height * scale,
            })
        })
        .collect()
}
