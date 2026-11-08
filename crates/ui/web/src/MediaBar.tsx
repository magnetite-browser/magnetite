import { Favicon } from "./Favicon";
import { Pause, PictureInPicture, Play, SkipBack, SkipForward, Speaker, SpeakerOff } from "./Icons";
import { hostOf, send, type MediaState, type TabItem, type TabRuntime } from "./bridge";

export interface MediaEntry {
  tab: TabItem;
  rt: TabRuntime;
  media: MediaState;
}

/**
 * Controls for media playing in tabs you are not looking at: one row per tab,
 * playing ones first. Clicking the source jumps to the tab.
 */
export function MediaBar({ entries }: { entries: MediaEntry[] }) {
  return (
    <div className="shrink-0 space-y-1.5 px-2 pt-2">
      {entries.map(({ tab, rt, media }) => {
        const title = media.title || tab.title || hostOf(tab.url);
        const label = media.artist ? `${title} · ${media.artist}` : title;
        return (
          <div key={tab.id} className="media no-drag">
            <button className="media-src" title={label} onClick={() => send({ cmd: "activate_tab", id: tab.id })}>
              <Favicon url={tab.url} src={rt.favicon} />
              <span className="min-w-0 flex-1 truncate">{title}</span>
            </button>
            <button className="ib sm" title="Previous" onClick={() => send({ cmd: "media", id: tab.id, action: "previous" })}><SkipBack width={14} height={14} /></button>
            <button className="ib sm" title={media.playing ? "Pause" : "Play"} onClick={() => send({ cmd: "media", id: tab.id, action: media.playing ? "pause" : "play" })}>
              {media.playing ? <Pause width={14} height={14} /> : <Play width={14} height={14} />}
            </button>
            <button className="ib sm" title="Next" disabled={!media.can_next} onClick={() => send({ cmd: "media", id: tab.id, action: "next" })}><SkipForward width={14} height={14} /></button>
            <button className="ib sm" title={rt.muted ? "Unmute" : "Mute"} onClick={() => send({ cmd: "set_muted", id: tab.id, muted: !rt.muted })}>
              {rt.muted ? <SpeakerOff width={14} height={14} /> : <Speaker width={14} height={14} />}
            </button>
            {media.video && (
              <button className="ib sm" title={media.pip ? "Back into the page" : "Picture in picture"} data-on={media.pip || undefined} onClick={() => send({ cmd: "picture_in_picture", id: tab.id })}>
                <PictureInPicture width={14} height={14} />
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
