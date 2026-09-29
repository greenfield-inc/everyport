import { notFound } from "next/navigation";
import { Og } from "../../../og.tsx";

// Only `pnpm og` reads this page, from the dev server.
export default function OgPage() {
  if (process.env.NODE_ENV === "production") notFound();
  return <Og />;
}
