---
name: api-change
description: Run the full API change workflow after modifying types in smelter-api. Generates schemas, TypeScript types, and verifies everything is in sync.
disable-model-invocation: false
allowed-tools: Bash, Read, Grep, Glob
---

Run the API change workflow. All steps must pass before the change is considered complete.

## Steps

1. **Generate JSON schemas from Rust types**
   Run: `cargo run -p tools --bin generate_from_types`
   This generates `tools/schemas/scene.schema.json` and `tools/schemas/api_types.schema.json`.

2. **Generate TypeScript types from schemas**
   Run in `./ts`: `pnpm run generate-types`
   This generates `ts/smelter/src/api.generated.ts`.

3. **Build the TypeScript SDK to verify compatibility**
   Run in `./ts`: `pnpm build:all`

4. **Show a summary of all generated/changed files** so the user can review what was affected.

5. **Try to update SDK**:
   Update TypeScript SDK code if the generated types require manual adaptation.
   In most cases you will need to:
   - add/modify type in `ts/smelter` package e.g. `ts/smelter/src/types/input.ts`
   - add/modify mapping in `ts/smelter-core` package e.g. `ts/smelter-core/src/api/input.ts`. In most cases it will be just switching snake case to camel case, but consider if there are more idiomatic alternatives.
   Inform user if it's not obvious how the changes should be adapted.
