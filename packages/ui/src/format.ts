// Text formats, ported from WhatThePort's Theme.swift `Format`. Memory uses
// binary units, matching Activity Monitor and "16 GB" on a 16 GB machine.

const MB = 1024 ** 2;
const GB = 1024 ** 3;

/** Exact reading for a server or process: "1.24 GB" or "612 MB". */
export function memoryParts(bytes: number): [string, string] {
  return bytes >= GB ? [(bytes / GB).toFixed(2), "GB"] : [(bytes / MB).toFixed(0), "MB"];
}

export const memory = (bytes: number) => memoryParts(bytes).join(" ");

/** Rounded amount for totals and growth: "4.9 GB", "2 GB" or "280 MB". */
export function totalParts(bytes: number): [string, string] {
  if (bytes < GB) return [(bytes / MB).toFixed(0), "MB"];
  const text = (bytes / GB).toFixed(1);
  return [text.endsWith(".0") ? text.slice(0, -2) : text, "GB"];
}

export const total = (bytes: number) => totalParts(bytes).join(" ");

/** "12m", "3h 12m" or "2d". */
export function duration(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  if (minutes < 1) return "<1m";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours >= 24) return `${Math.floor(hours / 24)}d`;
  const rest = minutes % 60;
  return rest ? `${hours}h ${rest}m` : `${hours}h`;
}

/** "12m", "3h" or "2d". */
export function shortDuration(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${Math.max(minutes, 1)}m`;
  const hours = Math.floor(minutes / 60);
  return hours < 24 ? `${hours}h` : `${Math.floor(hours / 24)}d`;
}

/** Whole numbers from 10% up, one decimal below, so an idle server reads "0.3%". */
export function percent(value: number): string {
  if (value <= 0) return "0%";
  if (value < 0.1) return "<0.1%";
  if (value < 10) return `${value.toFixed(1)}%`;
  return `${Math.round(value)}%`;
}

const clockFormat = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit" });
const secondsFormat = new Intl.DateTimeFormat(undefined, { hour: "numeric", minute: "2-digit", second: "2-digit" });
const dayFormat = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });

export const clock = (ms: number) => clockFormat.format(ms);
export const clockSeconds = (ms: number) => secondsFormat.format(ms);

/** "Today 10:12 AM", or "Sep 21, 10:12 AM" on another day than `now`. */
export function started(ms: number, now: number): string {
  const today = new Date(now).toDateString() === new Date(ms).toDateString();
  return today ? `Today ${clock(ms)}` : `${dayFormat.format(ms)}, ${clock(ms)}`;
}

/** Replaces the home folder with `~` on macOS, Linux and Windows paths. */
export function tilde(path: string): string {
  return path.replace(/^(\/Users\/[^/]+|\/home\/[^/]+|\/root|[A-Za-z]:\\Users\\[^\\]+)(?=[/\\]|$)/, "~");
}

/** Keeps the first and last two folders: "~/…/port-process-manager/providence". */
export function shortPath(path: string): string {
  const short = tilde(path);
  const separator = short.includes("\\") && !short.includes("/") ? "\\" : "/";
  const parts = short.split(separator).filter(Boolean);
  if (parts.length <= 3) return short;
  const head = short.startsWith(separator) ? `${separator}${parts[0]}` : parts[0];
  return [head, "…", ...parts.slice(-2)].join(separator);
}

/** The folder that holds `path`, shortened: "~/Sites". */
export function parentFolder(path: string): string {
  const short = tilde(path).replace(/[/\\]+$/, "");
  const cut = Math.max(short.lastIndexOf("/"), short.lastIndexOf("\\"));
  return cut > 0 ? short.slice(0, cut) : short;
}
