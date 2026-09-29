// The social preview, rendered by scripts/og.mjs from the page's own parts:
// the headline and one screen per OS.
import type { Os } from "@ppm/protocol";
import "@ppm/ui/styles.css";
import { createRoot } from "react-dom/client";
import { demoClient } from "./client.ts";
import { demoMachines } from "./machines.ts";
import { Screen } from "./Screen.tsx";
import { OS_NAMES } from "./sections.ts";
import "./site.css";

const SCREEN = { width: 480, height: 640 };
const SCALE = 0.55;

function Og() {
  return (
    <div className="og">
      <div className="og-head">
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

createRoot(document.getElementById("root")!).render(<Og />);
