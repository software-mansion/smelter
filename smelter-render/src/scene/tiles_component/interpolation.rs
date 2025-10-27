use std::collections::HashMap;

use crate::scene::{
    ComponentId,
    types::interpolation::{ContinuousValue, InterpolationState},
};

use super::tiles::Tile;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(super) enum TileId {
    ComponentId(ComponentId),
    Index(usize),
}

// #[derive(Clone, Copy)]
// struct TilePosition {
//     top:
// }

impl ContinuousValue for Vec<Option<Tile>> {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        let start_id_map: HashMap<&TileId, usize> = start
            .iter()
            .enumerate()
            .filter_map(|(index, tile)| tile.as_ref().map(|tile| (&tile.id, index)))
            .collect();

        if state.0 >= 1.0 {
            return end.clone();
        };

        end.iter()
            .enumerate()
            .map(|(end_index, tile)| {
                let tile = tile.as_ref()?;
                start_id_map
                    .get(&tile.id)
                    .and_then(|index| start.get(*index))
                    .and_then(|old_tile| {
                        old_tile
                            .as_ref()
                            .map(|old_tile| ContinuousValue::interpolate(old_tile, tile, state))
                    })
                    .or_else(|| {
                        if end_index < start.len() {
                            Some(ContinuousValue::interpolate(tile, tile, state))
                        } else {
                            None
                        }
                    })
            })
            .collect()
    }
}

impl ContinuousValue for Tile {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        Self {
            id: end.id.clone(),
            top: ContinuousValue::interpolate(&start.top, &end.top, state),
            left: ContinuousValue::interpolate(&start.left, &end.left, state),
            width: ContinuousValue::interpolate(&start.width, &end.width, state),
            height: ContinuousValue::interpolate(&start.height, &end.height, state),
        }
    }
}
