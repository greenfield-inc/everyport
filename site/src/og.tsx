// The social preview, rendered by scripts/og.mjs from the page's own parts:
// the wordmark, the headline and one screen per OS.
"use client";

import type { Os } from "@everyport/protocol";
import { demoClient } from "./client.ts";
import { demoMachines } from "./machines.ts";
import { Screen } from "./Screen.tsx";
import { OS_NAMES } from "./sections.ts";
import { Socket } from "./Socket.tsx";

const SCREEN = { width: 480, height: 640 };
const SCALE = 0.55;

export function Og() {
  return (
    <div className="og">
      <div className="og-head">
        <div className="og-brand">
          <Socket size={34} accent="var(--green)" />
          Everyport
        </div>
        <h1>
          Every dev server. <span className="accent">Every OS.</span>
        </h1>
        <p>macOS, Windows, Linux and the boxes you SSH into. Free and open source.</p>
      </div>
      <div className="og-screens">
        {(["macos", "windows", "linux"] as Os[]).map((os) => {
          const client = demoClient(demoMachines(os), false);
          return (
            <figure key={os}>
              <div style={{ width: SCREEN.width * SCALE, height: SCREEN.height * SCALE }}>
                <div style={{ transform: `scale(${SCALE})`, transformOrigin: "0 0" }}>
                  <Screen os={os} client={client} machine={client.machines()[0]} view={{ kind: "list" }} open onToggle={() => {}} {...SCREEN} />
                </div>
              </div>
              <figcaption>{OS_NAMES[os]}</figcaption>
            </figure>
          );
        })}
      </div>
    </div>
  );
}
