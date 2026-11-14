// The UI's icons: Lucide (ISC licence, bundled with the app), 16 px with a
// slightly lighter stroke than Lucide's default. Call sites use these names;
// to change an icon, change the mapping here.
import type { ComponentType } from "react";
import * as L from "lucide-react";
import type { LucideIcon, LucideProps } from "lucide-react";

/** Any icon of this file, for props and tables that take one. */
export type IconComponent = ComponentType<LucideProps>;

/** `Lucide` with our defaults; props given at the call site win. */
function icon(Lucide: LucideIcon, defaults: LucideProps = {}) {
  return (p: LucideProps) => <Lucide size={16} strokeWidth={1.75} {...defaults} {...p} />;
}

// Media controls read better filled at this size.
const filled: LucideProps = { fill: "currentColor" };

export const Back = icon(L.ChevronLeft);
export const Forward = icon(L.ChevronRight);
/** The arrow on a pinned tab that has left its page (Rows.tsx). */
export const HomeArrow = icon(L.ArrowLeft);
export const Reload = icon(L.RotateCw);
export const Reset = icon(L.RotateCcw);
export const SidebarIcon = icon(L.PanelLeft);
export const Close = icon(L.X);
/** The top bar's maximize button (Linux, Windows). */
export const Maximize = icon(L.Square, { size: 13 });
export const ChevronDown = icon(L.ChevronDown);
export const Plus = icon(L.Plus);
export const BookmarkIcon = icon(L.Bookmark);
export const KeepActiveIcon = icon(L.Zap);
export const Minus = icon(L.Minus);
export const Check = icon(L.Check);
export const Archive = icon(L.Archive);
export const HistoryIcon = icon(L.History);
export const DownloadIcon = icon(L.Download);
export const FileIcon = icon(L.File);
export const Reveal = icon(L.FolderSearch);
export const Search = icon(L.Search);
export const Arrow = icon(L.ArrowUpRight);
export const Command = icon(L.Command);
export const Dots = icon(L.Ellipsis);
export const Pin = icon(L.Pin);
export const Play = icon(L.Play, filled);
export const Pause = icon(L.Pause, filled);
export const SkipBack = icon(L.SkipBack, filled);
export const SkipForward = icon(L.SkipForward, filled);
export const Speaker = icon(L.Volume2);
export const SpeakerOff = icon(L.VolumeX);
export const PictureInPicture = icon(L.PictureInPicture2);
// Isolation levels: shared, separate, private.
export const SharedIcon = icon(L.Blend);
export const SeparateIcon = icon(L.SquareSplitHorizontal);
export const PrivateIcon = icon(L.EyeOff);
// Address row: site controls, link, connection problem.
export const SiteIcon = icon(L.SlidersHorizontal);
export const LinkIcon = icon(L.Link);
export const LockAlert = icon(L.ShieldAlert);
// "+" menu: new space, split, capture, new tab, smart folder.
export const SpacePlus = icon(L.CopyPlus);
export const SplitIcon = icon(L.Columns2);
export const ShareIcon = icon(L.Share);
export const SettingsIcon = icon(L.Settings2);
export const PaletteIcon = icon(L.Palette);
export const CookieIcon = icon(L.Cookie);
export const PuzzleIcon = icon(L.Puzzle);
export const DevicesIcon = icon(L.MonitorSmartphone);
export const PrivacyIcon = icon(L.ShieldCheck);
export const AdblockIcon = icon(L.ShieldBan);
export const InfoIcon = icon(L.Info);
export const GlobeIcon = icon(L.Globe);
export const NotesIcon = icon(L.ScrollText);
export const BugIcon = icon(L.Bug);
export const CodeIcon = icon(L.SquareCode);
export const CaptureIcon = icon(L.Focus);
export const TabPlus = icon(L.SquarePlus);
export const SmartFolderIcon = icon(L.FolderSync);
// Smart folders: refresh, problem, open elsewhere, account key.
export const RefreshIcon = icon(L.RefreshCw);
export const WarningIcon = icon(L.TriangleAlert);
export const ExternalIcon = icon(L.ExternalLink);
export const KeyIcon = icon(L.KeyRound);
// What a site asks for in a permission prompt (Notifications.tsx).
export const BellIcon = icon(L.Bell);
export const CameraIcon = icon(L.Camera);
export const CameraMoveIcon = icon(L.Move3d);
export const MicIcon = icon(L.Mic);
export const LocationIcon = icon(L.MapPin);
export const ClipboardIcon = icon(L.Clipboard);
export const MidiIcon = icon(L.Piano);
export const FontIcon = icon(L.Type);
export const IdleIcon = icon(L.Hourglass);
export const WindowsIcon = icon(L.AppWindow);
export const NetworkIcon = icon(L.Network);
export const SensorIcon = icon(L.Compass);
export const KeyboardIcon = icon(L.Keyboard);
export const PointerIcon = icon(L.MousePointer2);
export const QuestionIcon = icon(L.ShieldQuestionMark);

export const FolderIcon = ({ open, ...p }: LucideProps & { open?: boolean }) => {
  const Folder = open ? L.FolderOpen : L.Folder;
  return <Folder size={16} strokeWidth={1.75} {...p} />;
};
