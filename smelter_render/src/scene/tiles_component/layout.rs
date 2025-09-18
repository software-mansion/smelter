use std::time::Duration;

use crate::{
    scene::{layout::StatefulLayoutComponent, BorderRadius, RGBAColor, Size, StatefulComponent},
    transformations::layout::{LayoutContent, NestedLayout},
};

use super::tiles::Tile;

pub(super) fn layout_tiles(
    tiles: &[Option<Tile>],
    size: Size,
    children: &mut [StatefulComponent],
    pts: Duration,
    background_color: RGBAColor,
) -> NestedLayout {
    let children = children
        .iter_mut()
        .zip(tiles)
        .map(|(component, tile)| layout_child(component, tile.clone(), pts))
        .collect::<Vec<_>>();

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
        child_nodes_count: children.iter().map(|l| l.child_nodes_count).sum(),
        children,
        border_width: 0.0,
        border_color: RGBAColor(0, 0, 0, 0),
        border_radius: BorderRadius::ZERO,
        box_shadow: vec![],
    }
}

fn layout_child(child: &mut StatefulComponent, tile: Option<Tile>, pts: Duration) -> NestedLayout {
    let Some(tile) = tile else {
        // If tile does not need to be rendered we still need to create empty layout that has
        // child_nodes_count. Without this element children index offsets will not be calculated
        // correctly.
        let child_nodes_count = match child {
            StatefulComponent::Layout(layout_component) => layout_component.node_children().len(),
            _ => 1,
        };
        return NestedLayout::child_nodes_placeholder(child_nodes_count);
    };
    match child {
        StatefulComponent::Layout(layout_component) => {
            let children_layouts = layout_component.layout(
                Size {
                    width: tile.width,
                    height: tile.height,
                },
                pts,
            );
            NestedLayout {
                top: tile.top,
                left: tile.left,
                width: tile.width,
                height: tile.height,
                rotation_degrees: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                crop: None,
                mask: None,
                content: LayoutContent::None,
                child_nodes_count: children_layouts.child_nodes_count,
                children: vec![children_layouts],
                border_width: 0.0,
                border_color: RGBAColor(0, 0, 0, 0),
                border_radius: BorderRadius::ZERO,
                box_shadow: vec![],
            }
        }
        _ => {
            let fitted = fit_into_tile(tile, child, pts);
            NestedLayout {
                // TODO: fit
                top: fitted.top,
                left: fitted.left,
                width: fitted.width,
                height: fitted.height,
                rotation_degrees: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                crop: None,
                mask: None,
                content: StatefulLayoutComponent::layout_content(child, 0),
                child_nodes_count: 1,
                children: vec![],
                border_width: 0.0,
                border_color: RGBAColor(0, 0, 0, 0),
                border_radius: BorderRadius::ZERO,
                box_shadow: vec![],
            }
        }
    }
}

fn fit_into_tile(tile: Tile, component: &StatefulComponent, pts: Duration) -> Tile {
    let Some(width) = component.width(pts) else {
        return tile;
    };
    let Some(height) = component.height(pts) else {
        return tile;
    };
    let scale_to_fit_width = tile.width / width;
    let scale_to_fit_height = tile.height / height;
    let scale_factor = f32::min(scale_to_fit_width, scale_to_fit_height);

    let top_offset = (tile.height - scale_factor * height) / 2.0;
    let left_offset = (tile.width - scale_factor * width) / 2.0;

    Tile {
        top: tile.top + top_offset,
        left: tile.left + left_offset,
        width: scale_factor * width,
        height: scale_factor * height,
        id: tile.id,
    }
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
