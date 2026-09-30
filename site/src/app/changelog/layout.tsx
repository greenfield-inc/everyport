import type { ReactNode } from "react";
import { DocsShell } from "../../DocsShell.tsx";

export default function ChangelogLayout({ children }: { children: ReactNode }) {
  return <DocsShell>{children}</DocsShell>;
}
