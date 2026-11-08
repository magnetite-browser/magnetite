/**
 * `items` in runs of the same local day ("Monday, Nov 10"), in their order;
 * `at` is an item's time in Unix seconds. The archive and downloads lists.
 */
export function groupByDay<T>(items: readonly T[], at: (item: T) => number): [string, T[]][] {
  const groups: [string, T[]][] = [];
  for (const it of items) {
    const day = new Date(at(it) * 1000).toLocaleDateString(undefined, { weekday: "long", month: "short", day: "numeric" });
    const last = groups[groups.length - 1];
    if (last && last[0] === day) last[1].push(it);
    else groups.push([day, [it]]);
  }
  return groups;
}
