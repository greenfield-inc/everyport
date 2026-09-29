import { defineConfig } from "vitest/config";

// The fixture's times read as Paper's ("Today 10:12 AM") in Los Angeles.
export default defineConfig({
  test: { include: ["src/**/*.test.tsx"], env: { TZ: "America/Los_Angeles" } },
});
