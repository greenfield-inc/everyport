// The ppm wire protocol for TypeScript clients. Types in ./generated come from
// crates/ppm-core/src/protocol.rs; run `pnpm --filter @ppm/protocol generate`
// after changing it.
export * from "./generated/index.ts";
export * from "./client.ts";

import type { Snapshot } from "./generated/index.ts";
import snapshot from "../fixtures/snapshot.json" with { type: "json" };

/** The Paper design's list state, as protocol data. Used by UI dev and tests. */
export const fixtureSnapshot = snapshot as Snapshot;
