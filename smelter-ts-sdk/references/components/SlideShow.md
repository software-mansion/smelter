# SlideShow Component

Chains a series of scenes sequentially. Accepts only `<Slide>` components as children. After mounting, displays slides one after another.

## Slide Duration Logic

1. If `durationMs` is specified on the `<Slide>`, it takes precedence.
2. If any descendant component is `InputStream`, `Mp4`, or another `SlideShow`, the slide stays until those finish.
3. Otherwise, switches after 1 second.

## Type Definitions

```tsx
type SlideShowProps = {
    children?: ReactNode;  // Must be <Slide> components
}

type SlideProps = {
    children: ReactNode;
    durationMs?: number;
}
```

## SlideShow Props

### children
List of `<Slide />` components.
- **Type**: `ReactNode`

## Slide Props

### children
Content of the slide.
- **Type**: `ReactNode`

### durationMs
How long to show this slide (overrides automatic detection).
- **Type**: `number`

## Example

```tsx
<SlideShow>
  <Slide durationMs={3000}>
    <Text style={{ fontSize: 48 }}>Intro</Text>
  </Slide>
  <Slide>
    <InputStream inputId="video1" /> {/* stays until video ends */}
  </Slide>
  <Slide durationMs={5000}>
    <Image source="outro.jpg" />
  </Slide>
</SlideShow>
```
