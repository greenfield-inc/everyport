"use client";

import dynamic from "next/dynamic";

/**
 * The page renders in the browser only: it picks the visitor's OS and a desk or
 * phone layout from the window before its first paint.
 */
export const Landing = dynamic(() => import("./App.tsx").then((module) => module.App), { ssr: false });
