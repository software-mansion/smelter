# Tiles Component

A layout component that arranges all children side-by-side in equal-sized, non-overlapping tiles, automatically calculating optimal rows/columns.

## Type Definition

```tsx
type TilesProps = {
  id?: string;
  children: ReactNode;
  style?: TilesStyleProps;
  transition?: Transition;
}
```

## Props

### children (required)
Each child is placed in one tile.
- **Type**: `ReactNode`

### id
Component ID.
- **Type**: `string`
- **Default**: Value from `useId` hook

### style
Layout styling.
- **Type**: `TilesStyleProps` — see `references/props/TilesStyleProps.md`

### transition
Controls animation when tiles are added, removed, or reordered. Does NOT animate size changes like `View`.
- **Type**: `Transition` — see `references/props/Transition.md`

## Positioning Constraints

- **Children**: Tiles do NOT support absolute positioning for children (top/left/right/bottom/rotation are ignored).
- **Self**: Tiles CANNOT be absolutely positioned relative to its parent.

## Child Placement

| Child Type | Behavior |
|---|---|
| Non-layout component (e.g., InputStream) | Scales proportionally to fit tile, centered if aspect ratios differ |
| Layout component (e.g., View, Rescaler) | Gets full tile width/height, ignoring its own width/height |

## Tile Layout Calculation

Rows/columns determined by:
1. Size of the `Tiles` component
2. `tileAspectRatio` (default `16:9`)
3. Number of children

Tiles are placed left-to-right, top-to-bottom.

## Transition Behavior

- **Adding a child**: Existing tiles shift to new positions; new child appears without animation.
- **Removing a child**: Removed tile disappears immediately; remaining tiles shift to fill gaps.
- **Reordering**: Tiles animate to new positions.
