# Media controls and picture-in-picture

Magnetite knows what each tab plays. A tab you are not looking at gets a
row of controls in the sidebar, a tab with sound gets a mute button, and a
playing video follows you into a floating window when you leave its tab.

## Media rows

A row appears above the favorites for each tab that has media and is not on
screen, in any space. There are at most two rows, playing ones first.

| Control | Action |
|---|---|
| Source (icon and title) | Jumps to the tab |
| Previous | The page's "previous track" action; without one, back to the start |
| Play / Pause | Through the page's own handler when it has one |
| Next | Only when the page offers a "next track" action |
| Mute | Mutes or unmutes the tab |
| Picture-in-picture | Videos only: pops the video out, or puts it back |

- The title is the page's Media Session title, else the tab's title. The
  tooltip adds the artist.
- A tab row shows a speaker button while the tab plays or is muted; it
  mutes and unmutes the tab. The mute is forgotten when the tab is
  unloaded.

## What counts as media

- An audio or video element that is audible (not muted, volume above zero)
  and really played its current source.
- Paused media keeps its row, so it can be resumed. Media that was loaded
  but never started (a queued next video, an ad in a background tab) gets
  none. Ended media loses its row.
- Tabs with media, playing or paused, are never unloaded or auto-archived
  in the background.

## Picture-in-picture

- Settings → General → "Pop out playing videos" (on by default): when a tab
  with a playing video leaves the screen, the video continues in a floating
  window. A page that stays on screen in a split keeps its video.
- Showing the tab again puts the video back into the page, whatever the
  setting says.
- The picture-in-picture button of a media row does both by hand.

On macOS the floating video has Magnetite's own controls:

| Control | Action |
|---|---|
| Back to Tab | Shows the tab, puts the video back into the page and brings Magnetite to the front |
| Minimize | The window goes; the video keeps playing in its tab |
| Close | Pauses the video; the window goes |
| 15 s back, 15 s forward | Seek; live streams have no seek buttons |
| Play / Pause | As in the media row |

- The controls show while the pointer is over the video and briefly when it
  pops out. They hide when the pointer rests for 2.5 s. A narrow window
  shows the top buttons as icons only.
- Dragging the video moves the window. Dragging an edge or corner resizes
  it with the video's proportions, never narrower than 160 pt or wider than
  the screen. A two-finger swipe flings it to the screen edge in that
  direction; a diagonal swipe reaches a corner.
- The video window shows no page address.

## How it works

- `media_hook.js` runs in the main frame of every web page (http, https
  and file addresses), in the page's own JavaScript world and in no other
  (not in the worlds extensions run content scripts in), before the page's
  scripts. It wraps `MediaSession.setActionHandler` and
  `HTMLMediaElement.play` to learn the page's handlers and media elements,
  and reports `{playing, title, artist, video, pip, live, can_next,
  can_prev}` on change through a native function only its closure can
  reach. It adds nothing to the page's globals.
- Commands go back to the renderer as a process message. It calls the
  closure's `control` function: the page's own Media Session handler when
  there is one, else the element.
- Entering picture-in-picture needs a user gesture, so the engine runs
  `Runtime.evaluate` with `userGesture` on CEF's in-process DevTools
  channel. Mute is `CefBrowserHost::SetAudioMuted`; a page muted before it
  has its window is muted as soon as it has one.
- The macOS controls are a nearly clear panel that lies over Chromium's
  video window as its child window. It takes the pointer, so Chromium's own
  controls never show. Chromium would show the page address until the user
  clicks into its window; the feature
  `VideoPipForceTrustedForMediaPlaybackForTesting`, enabled at start, keeps
  it off. As its name says, the feature is meant for testing, so a later
  Chromium version may drop it.
- Reports are untrusted, because the page can shape them. They are capped
  at 4 KiB and built on objects without a prototype. A page gets the
  floating controls only if it has them already, Magnetite asked it into
  picture-in-picture in the last five seconds, or it is on screen
  (`pip::may_own`). A background page cannot claim another page's video.

## Data

Nothing leaves the machine. Title and artist come from the page's own Media
Session metadata, are cut to 200 characters and live only in memory, like
the mute state. The pop-out setting is `settings.auto_pip` in
`<data-dir>/state.json`; it is not synced.

## Not built yet

- Controls for media in iframes (embedded players). The hook runs in main
  frames only.

Code: `crates/engine/src/media.rs`, `media_hook.js`, `pip.rs`, `cef_app.rs`
(the feature switch) and `mac/mod.rs` (swipe); `crates/app/src/app/tabs.rs`
(`follow_pictures`, `has_media`); `crates/ui/web/src/MediaBar.tsx` and
`Rows.tsx`.
