import type { Machine, Os } from "@everyport/protocol";
import { MachineSwitcher, Themed } from "@everyport/ui";
import { type CSSProperties, useCallback, useEffect, useRef, useState, useSyncExternalStore } from "react";
import { demoClient } from "./client.ts";
import { Credit, SectionCopy } from "./Copy.tsx";
import { cliName, demoMachines, LOCAL } from "./machines.ts";
import { usePill } from "./pill.ts";
import { Screen } from "./Screen.tsx";
import { Socket } from "./Socket.tsx";
import { OS_NAMES, REPO, SECTIONS, type SectionId, visitorOs } from "./sections.ts";

const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max);

function useMedia(query: string) {
  return useSyncExternalStore(
    (change) => {
      const list = matchMedia(query);
      list.addEventListener("change", change);
      return () => list.removeEventListener("change", change);
    },
    () => matchMedia(query).matches,
  );
}

const reducedMotion = () => matchMedia("(prefers-reduced-motion: reduce)").matches;

/** Scales a fixed-size child to fit its box, never above 1. Without a height, it fits the width only. */
function useFit(width: number, height = 0) {
  const box = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);
  useEffect(() => {
    const element = box.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => setScale(Math.min(1, entry.contentRect.width / width, height ? entry.contentRect.height / height : 1)));
    observer.observe(element);
    return () => observer.disconnect();
  }, [width, height]);
  return { box, scale };
}

function Brand() {
  return (
    <a className="brand" href="#" onClick={() => scrollTo({ top: 0 })}>
      <Socket size={22} accent="var(--green)" />
      Everyport
    </a>
  );
}

function OsSwitcher({ os, onOs }: { os: Os; onOs: (os: Os) => void }) {
  return (
    <div ref={usePill(os)} role="radiogroup" aria-label="Operating system" className="segmented">
      {(Object.keys(OS_NAMES) as Os[]).map((name) => (
        <button key={name} type="button" role="radio" aria-checked={name === os} onClick={() => onOs(name)}>
          {OS_NAMES[name]}
        </button>
      ))}
    </div>
  );
}

function Controls({ os, onOs, machines, machine, onMachine, changed, onReset }: ControlProps) {
  const name = cliName[machine];
  const count = machines.find((candidate) => candidate.id === machine)?.snapshot?.servers.length ?? 0;
  return (
    <div className="controls">
      <OsSwitcher os={os} onOs={onOs} />
      <div ref={usePill(`${machine}-${machines.length}`, "[role=tablist]")} className="machines">
        <Themed appearance="dark">
          <MachineSwitcher machines={machines} current={machine} onSelect={onMachine} />
        </Themed>
      </div>
      <p className="cli-line">
        <code>
          <span aria-hidden>$ </span>everyport {name ? `--on ${name} ` : ""}list
        </code>
        <span>
          {count} {count === 1 ? "server" : "servers"}
        </span>
        {changed && (
          <button type="button" className="reset" onClick={onReset}>
            Reset demo
          </button>
        )}
      </p>
    </div>
  );
}

type ControlProps = {
  os: Os;
  onOs: (os: Os) => void;
  machines: Machine[];
  machine: string;
  onMachine: (id: string) => void;
  changed: boolean;
  onReset: () => void;
};

const DESK = { width: 880, height: 680 };
const PHONE = { width: 440, height: 700 };

export function App() {
  const [os, setOs] = useState<Os>(visitorOs);
  const [client] = useState(() => demoClient(demoMachines(os), true));
  const [machines, setMachines] = useState(() => client.machines());
  useEffect(() => client.subscribe(setMachines), [client]);
  const [machineId, setMachineId] = useState(LOCAL);
  const [active, setActive] = useState(0);
  const [open, setOpen] = useState(true);
  const [override, setOverride] = useState<{ section: number; port: number } | null>(null);
  const [picked, setPicked] = useState(false);
  const desk = useMedia("(min-width: 900px) and (min-aspect-ratio: 1/1)");

  const changeOs = (next: Os) => {
    const nextMachines = demoMachines(next);
    setOs(next);
    client.reset(nextMachines);
    if (!nextMachines.some((machine) => machine.id === machineId)) setMachineId(LOCAL);
  };
  const pickMachine = (id: string) => {
    setPicked(true);
    setMachineId(id);
  };

  const section = SECTIONS[active];
  // Sections about this computer's servers show it, and so does every
  // section after Machines walked through them, unless the visitor picked one.
  useEffect(() => {
    setOverride(null);
    setOpen(true);
    if (section.local || (section.id !== "machines" && !picked)) setMachineId(LOCAL);
  }, [section]);

  // The Machines section walks through every machine until the visitor picks one.
  useEffect(() => {
    if (section.id !== "machines" || picked || reducedMotion()) return;
    const ids = machines.map((machine) => machine.id);
    const timer = setInterval(() => setMachineId((current) => ids[(ids.indexOf(current) + 1) % ids.length]), 2200);
    return () => clearInterval(timer);
  }, [section, machines.length, picked]);

  const controls: ControlProps = {
    os,
    onOs: changeOs,
    machines,
    machine: machineId,
    onMachine: pickMachine,
    changed: client.changed(),
    onReset: () => client.reset(demoMachines(os)),
  };
  const screenFor = (index: number, size: { width: number; height: number }) => {
    const current = SECTIONS[index];
    const view = override?.section === index ? ({ kind: "detail", port: override.port } as const) : current.view;
    return (
      <Screen
        os={os}
        client={client}
        machine={machines.find((machine) => machine.id === (current.local ? LOCAL : machineId))}
        view={view}
        open={open}
        onToggle={() => setOpen(!open)}
        alert={current.id === "leaks" && override?.section !== index ? { onDetails: (port) => setOverride({ section: index, port }) } : undefined}
        {...size}
      />
    );
  };

  return (
    <main style={{ ["--accent" as string]: section.accent } as CSSProperties}>
      {desk ? (
        <Desk active={active} setActive={setActive} controls={controls} screen={screenFor(active, DESK)} os={os} onOs={changeOs} />
      ) : (
        <Stacked controls={controls} screenFor={(index) => screenFor(index, PHONE)} os={os} onOs={changeOs} />
      )}
      <footer>
        <Credit />
        <nav aria-label="Footer">
          <a href={REPO}>GitHub</a>
          <a href={`${REPO}/tree/main/docs`}>Docs</a>
          <a href={`${REPO}/releases`}>Releases</a>
          <a href={`${REPO}/blob/main/LICENSE`}>License</a>
        </nav>
      </footer>
    </main>
  );
}

function Header({ active, onNavigate }: { active?: number; onNavigate?: (id: SectionId) => void }) {
  const pill = usePill(active);
  return (
    <header className="header">
      <Brand />
      {onNavigate && (
        <nav ref={pill} aria-label="Sections">
          {SECTIONS.map((section, index) => (
            <a
              key={section.id}
              href={`#${section.id}`}
              aria-current={index === active ? "true" : undefined}
              onClick={(event) => {
                event.preventDefault();
                onNavigate(section.id);
              }}
            >
              {section.nav}
            </a>
          ))}
        </nav>
      )}
      <a className="github" href={REPO}>
        GitHub
      </a>
    </header>
  );
}

function Desk({
  active,
  setActive,
  controls,
  screen,
  os,
  onOs,
}: {
  active: number;
  setActive: (index: number) => void;
  controls: ControlProps;
  screen: React.ReactNode;
  os: Os;
  onOs: (os: Os) => void;
}) {
  const track = useRef<HTMLDivElement>(null);
  const copy = useRef<HTMLDivElement>(null);
  const { box, scale } = useFit(DESK.width, DESK.height);

  useEffect(() => {
    const element = track.current;
    const blocks = [...(copy.current?.children ?? [])] as HTMLElement[];
    if (!element) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      const active = Math.round(clamp(-element.getBoundingClientRect().top / innerHeight, 0, SECTIONS.length - 1));
      blocks.forEach((block, index) => {
        block.toggleAttribute("data-active", index === active);
        block.inert = index !== active;
      });
      setActive(active);
    };
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    update();
    addEventListener("scroll", onScroll, { passive: true });
    addEventListener("resize", onScroll);
    return () => {
      cancelAnimationFrame(frame);
      removeEventListener("scroll", onScroll);
      removeEventListener("resize", onScroll);
    };
  }, [setActive]);

  const navigate = useCallback((id: SectionId, instant = false) => {
    const element = track.current;
    if (!element) return;
    const index = SECTIONS.findIndex((section) => section.id === id);
    const top = element.getBoundingClientRect().top + scrollY + index * innerHeight;
    scrollTo({ top, behavior: instant || reducedMotion() ? "auto" : "smooth" });
    history.replaceState(null, "", `#${id}`);
  }, []);

  // Links such as /#terminal open on that section.
  useEffect(() => {
    const id = location.hash.slice(1);
    if (SECTIONS.some((section) => section.id === id)) navigate(id as SectionId, true);
  }, [navigate]);

  return (
    <div ref={track} className="track" style={{ height: `${SECTIONS.length * 100}vh` }}>
      <div className="stage">
        <Header active={active} onNavigate={navigate} />
        <div className="stage-body">
          <div ref={copy} className="copy">
            {SECTIONS.map((section) => (
              <section key={section.id} id={section.id} className="copy-block">
                <SectionCopy id={section.id} os={os} onOs={onOs} onNavigate={navigate} />
              </section>
            ))}
          </div>
          <div className="stage-right">
            <Controls {...controls} />
            <div ref={box} className="fit">
              <div className="fit-inner" style={{ width: DESK.width * scale, height: DESK.height * scale }}>
                <div style={{ transform: `scale(${scale})`, transformOrigin: "0 0" }}>{screen}</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

/**
 * Renders its children only while on screen, so a phone runs one or two
 * popovers instead of six, and their rows' ids stay unique.
 */
function OnScreen({ children, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  const box = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    const observer = new IntersectionObserver(([entry]) => setVisible(entry.isIntersecting));
    observer.observe(box.current!);
    return () => observer.disconnect();
  }, []);
  return (
    <div ref={box} {...props}>
      {visible && children}
    </div>
  );
}

function Stacked({ controls, screenFor, os, onOs }: { controls: ControlProps; screenFor: (index: number) => React.ReactNode; os: Os; onOs: (os: Os) => void }) {
  const { box, scale } = useFit(PHONE.width);
  const navigate = (id: SectionId) => document.getElementById(id)?.scrollIntoView({ behavior: reducedMotion() ? "auto" : "smooth" });
  return (
    <div className="stacked">
      <Header />
      <div className="stacked-controls">
        <Controls {...controls} />
      </div>
      {SECTIONS.map((section, index) => (
        <section key={section.id} id={section.id} className="stacked-section">
          <div className="copy-block">
            <SectionCopy id={section.id} os={os} onOs={onOs} onNavigate={navigate} />
          </div>
          {section.id !== "install" && (
            <div ref={index === 0 ? box : undefined} className="fit">
              <OnScreen className="fit-inner" style={{ width: PHONE.width * scale, height: PHONE.height * scale }}>
                <div style={{ transform: `scale(${scale})`, transformOrigin: "0 0" }}>{screenFor(index)}</div>
              </OnScreen>
            </div>
          )}
        </section>
      ))}
    </div>
  );
}
