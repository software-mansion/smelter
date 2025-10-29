use std::collections::{HashMap, HashSet};

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

impl ContinuousValue for Vec<Option<Tile>> {
    fn interpolate(start: &Self, end: &Self, state: InterpolationState) -> Self {
        let start_id_map: HashMap<&TileId, usize> = start
            .iter()
            .enumerate()
            .filter_map(|(index, tile)| tile.as_ref().map(|tile| (&tile.id, index)))
            .collect();
        let end_id_set: HashSet<&TileId> = end
            .iter()
            .filter_map(|tile| tile.as_ref().map(|tile| &tile.id))
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
                        // Animation interpolation. Returns `Some(_)` only if for the tile from the `end` state
                        // exists a tile with the same ID in the `start` state.
                        old_tile
                            .as_ref()
                            .map(|old_tile| ContinuousValue::interpolate(old_tile, tile, state))
                    })
                    .or_else(|| {
                        // This closure prevents tile from displaying empty square for the duration
                        // of transition if the tile is swapped "in place" (i.e. no animation
                        // occurs).
                        start
                            .iter()
                            .flatten()
                            .find(|start_tile| are_positions_equal(start_tile, tile))
                            .and_then(|start_tile| match end_id_set.contains(&start_tile.id) {
                                true => None,
                                false => Some(tile.clone()),
                            })
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

fn are_positions_equal(lhs: &Tile, rhs: &Tile) -> bool {
    const TOLERANCE: f32 = 0.001;

    let top_eq = f32::abs(lhs.top - rhs.top) <= TOLERANCE;
    let left_eq = f32::abs(lhs.left - rhs.left) <= TOLERANCE;
    let width_eq = f32::abs(lhs.width - rhs.width) <= TOLERANCE;
    let height_eq = f32::abs(lhs.height - rhs.height) <= TOLERANCE;

    top_eq && left_eq && width_eq && height_eq
}
