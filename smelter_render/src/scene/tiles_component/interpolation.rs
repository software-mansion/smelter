use std::collections::HashMap;

use crate::scene::{
    types::interpolation::{ContinuousValue, InterpolationState},
    ComponentId,
};

use super::tiles::Tile;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(super) enum TileId {
    ComponentId(ComponentId),
    Index(usize),
}

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
            .map(|tile| {
                let tile = tile.as_ref()?;
                start_id_map
                    .get(&tile.id)
                    .and_then(|index| start.get(*index))
                    .and_then(|old_tile| {
                        old_tile
                            .as_ref()
                            .map(|old_tile| ContinuousValue::interpolate(old_tile, tile, state))
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
