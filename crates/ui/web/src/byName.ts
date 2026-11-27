/**
 * The items whose name contains `query` (case aside): those first whose name
 * starts with it, then those where a word does, then the rest; otherwise in
 * their own order. Nothing typed keeps them all. Used where a list is
 * narrowed by typing: the list of all spaces, a menu's list with a field.
 */
export function byName<T>(items: readonly T[], query: string, name: (item: T) => string): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...items];
  const rank = (item: T) => {
    const title = name(item).toLowerCase();
    if (title.startsWith(q)) return 0;
    let found = false;
    // Every place the text is at: one of them may start a word.
    for (let at = title.indexOf(q); at >= 0; at = title.indexOf(q, at + 1)) {
      if (/\s/.test(title.charAt(at - 1))) return 1;
      found = true;
    }
    return found ? 2 : null;
  };
  return items
    .map((item) => ({ item, rank: rank(item) }))
    .filter((m): m is { item: T; rank: number } => m.rank !== null)
    .sort((a, b) => a.rank - b.rank)
    .map((m) => m.item);
}
