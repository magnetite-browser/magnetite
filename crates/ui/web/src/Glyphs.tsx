// Monochrome space glyphs (Lucide line icons), tinted with the space colour.
// Imported icon names (and a few emoji) map onto this set; the names are
// stored with spaces, so keep them when changing an icon. `IconPicker` picks
// one for a space or folder.
import * as L from "lucide-react";
import type { LucideIcon, LucideProps } from "lucide-react";
import { pageZoom } from "./bridge";
import { useDismiss } from "./dismiss";

const ICONS: Record<string, LucideIcon> = {
  bolt: L.Zap,
  home: L.House,
  briefcase: L.Briefcase,
  book: L.BookOpen,
  code: L.Code,
  globe: L.Globe,
  heart: L.Heart,
  star: L.Star,
  music: L.Music,
  cart: L.ShoppingCart,
  camera: L.Camera,
  flask: L.FlaskConical,
  leaf: L.Leaf,
  flame: L.Flame,
  chat: L.MessageSquare,
  wrench: L.Wrench,
  compass: L.Compass,
  folder: L.Folder,
  plane: L.Plane,
  gamepad: L.Gamepad2,
  shield: L.Shield,
  medical: L.ShieldPlus,
  github: L.GitBranch,
  diamond: L.Diamond,
  circle: L.Circle,
  square: L.Square,
  triangle: L.Triangle,
};

export const GLYPH_NAMES = Object.keys(ICONS);

const EMOJI_TO_GLYPH: Record<string, string> = { "⚡": "bolt", "🏠": "home", "💼": "briefcase", "📚": "book", "🎨": "flame", "🎧": "music", "🛠️": "wrench", "🧪": "flask", "🌍": "globe", "🛒": "cart", "💬": "chat", "📷": "camera", "🎮": "gamepad", "✈️": "plane", "🌿": "leaf", "🔥": "flame", "💡": "bolt", "🧭": "compass", "🗂️": "folder", "✦": "star", "◆": "diamond", "●": "circle", "▲": "triangle", "■": "square", "🏥": "medical", "🐙": "github", "❤️": "heart", "⭐": "star" };
const NAME_ALIASES: Record<string, string> = { flash: "bolt", work: "briefcase", terminal: "code", gamecontroller: "gamepad", paintbrush: "flame", graduationcap: "book", film: "camera", airplane: "plane" };

/** Resolve an icon value (glyph name, an imported icon name or emoji) to a glyph name. */
export function glyphFor(icon?: string): string | null {
  if (!icon) return null;
  if (ICONS[icon]) return icon;
  const lower = icon.toLowerCase();
  if (NAME_ALIASES[lower]) return NAME_ALIASES[lower];
  if (ICONS[lower]) return lower;
  return EMOJI_TO_GLYPH[icon] ?? null;
}

export function Glyph({ name, size = 14, ...rest }: { name: string; size?: number } & LucideProps) {
  const Icon = ICONS[name] ?? L.Circle;
  return <Icon size={size} strokeWidth={1.75} {...rest} />;
}

/** A space's icon: glyph when known, otherwise its initial. Always one colour. */
export function SpaceGlyph({ icon, title, size = 14 }: { icon?: string; title: string; size?: number }) {
  const g = glyphFor(icon);
  if (g) return <Glyph name={g} size={size} />;
  return <span style={{ fontSize: size - 2, fontWeight: 600, lineHeight: 1 }}>{(icon && [...icon].length <= 2 ? icon : title.charAt(0)).toUpperCase()}</span>;
}

/**
 * Grid of glyphs for a space or folder icon; `allowDefault` adds a button
 * that brings the default icon back. Closes on Esc, a click outside or a
 * click into the page.
 */
export function IconPicker({ at, current, allowDefault, onPick, onClose }: { at: { x: number; y: number }; current?: string; allowDefault?: boolean; onPick: (glyph: string | null) => void; onClose: () => void }) {
  useDismiss(onClose);
  // `at` and the window in the picker's own px (see `pageZoom`).
  const zoom = pageZoom();
  const left = Math.max(0, Math.min(at.x / zoom, window.innerWidth / zoom - 232));
  const top = Math.min(at.y / zoom, window.innerHeight / zoom - 150);
  return (
    <div className="cmdbar-backdrop" onMouseDown={onClose}>
      <div className="menu no-drag" style={{ left, top, minWidth: 0, maxWidth: "none", width: 232 }} onMouseDown={(e) => e.stopPropagation()}>
        <div className="grid grid-cols-8 gap-0.5">
          {GLYPH_NAMES.map((g) => (
            <button key={g} className="justify-center rounded-md hover:bg-white/10" style={{ padding: 0, height: 28, width: 26, color: g === current ? "var(--space)" : "var(--secondary)" }} title={g} onClick={() => onPick(g)}><Glyph name={g} size={15} /></button>
          ))}
        </div>
        {allowDefault && current && (
          <>
            <hr />
            <button className="w-full text-left" onClick={() => onPick(null)}>Default icon</button>
          </>
        )}
      </div>
    </div>
  );
}
