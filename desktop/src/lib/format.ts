import type { ItemView, StatusValue } from "./types";

/** `2026-11-11T10:30:00` → `11/11/2026 10:30`; `2026-11-11` → `11/11/2026`. */
export function formatDate(iso: string | null | undefined): string {
  if (!iso) return "";
  const m = /^(\d{4})-(\d{2})-(\d{2})(?:T(\d{2}):(\d{2}))?/.exec(iso);
  if (!m) return iso;
  const day = `${m[3]}/${m[2]}/${m[1]}`;
  return m[4] ? `${day} ${m[4]}:${m[5]}` : day;
}

/** `2026-11-23` → `23/11/26` (the control date in the list). */
export function shortDate(iso: string | null | undefined): string {
  const m = iso ? /^(\d{4})-(\d{2})-(\d{2})/.exec(iso) : null;
  return m ? `${m[3]}/${m[2]}/${m[1].slice(2)}` : "";
}

/** Relative day for lists: "hoje", "ontem", or the date. */
export function relativeDay(isoDate: string): string {
  const today = new Date();
  const d = new Date(`${isoDate.slice(0, 10)}T00:00:00`);
  const t = new Date(today.getFullYear(), today.getMonth(), today.getDate());
  const diff = Math.round((t.getTime() - d.getTime()) / 86_400_000);
  if (diff === 0) return "hoje";
  if (diff === 1) return "ontem";
  return formatDate(isoDate);
}

export function todayIso(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Parse "1,3-5" into a set of numbers within [lo, hi]; null if invalid or empty. */
export function parseRanges(text: string, lo = 1, hi = 5): Set<number> | null {
  const values = new Set<number>();
  for (const raw of text.replace(/;/g, ",").split(",")) {
    const part = raw.trim();
    if (!part) continue;
    const m = /^(\d+)(?:\s*-\s*(\d+))?$/.exec(part);
    if (!m) return null;
    let a = Number(m[1]);
    let b = m[2] ? Number(m[2]) : a;
    if (a > b) [a, b] = [b, a];
    if (a < lo || b > hi) return null;
    for (let i = a; i <= b; i++) values.add(i);
  }
  return values.size ? values : null;
}

export function formatRanges(values: Iterable<number>): string {
  const nums = [...new Set(values)].sort((a, b) => a - b);
  const parts: string[] = [];
  let i = 0;
  while (i < nums.length) {
    let j = i;
    while (j + 1 < nums.length && nums[j + 1] === nums[j] + 1) j++;
    parts.push(i === j ? String(nums[i]) : `${nums[i]}-${nums[j]}`);
    i = j + 1;
  }
  return parts.join(",");
}

export const CLOSED: StatusValue[] = ["done", "discarded"];

export function compareKeys(a: ItemView, b: ItemView): number {
  return a.number - b.number || a.addendum - b.addendum;
}

export function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}
