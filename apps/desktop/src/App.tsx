import { fixtureSnapshot } from "@ppm/protocol";

export function App() {
  return <pre>{fixtureSnapshot.servers.map((s) => `:${s.port} ${s.project.name}`).join("\n")}</pre>;
}
