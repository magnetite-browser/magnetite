# Closing and quitting

Closing a window and quitting Magnetite are two things. On macOS closing the
last window leaves Magnetite running, and one stray `Cmd+Q` ends nothing.

## Closing the last window (macOS)

The window's close button, or Close in a menu, takes the last window off the
screen. Magnetite runs on without one:

- Its pages stay as they are: loaded tabs stay loaded, so what plays goes on
  playing and downloads go on. Mini windows are windows of their own and
  stay open.
- The state is saved at that moment, as it is before quitting.
- The window comes back as it was with a click on the Dock icon and with a
  command from the menu bar or its keys (`Cmd+T`, `Cmd+N`, Settings and the
  others). "New Window", "Close Window" and "Close Tab" then only bring the
  window back.
- A link from another app brings the window back when it opens as a tab or
  a peek. One that opens in a mini window (the default, `mini-window.md`)
  leaves the main window closed.

With several windows open, closing one of them works as before: its pages go
to the window that is current next. Only the last one stays in the
background.

On Linux and Windows closing the last window quits Magnetite.

## Quitting

| Way | What happens |
|---|---|
| "Quit Magnetite" in the menu bar, chosen with the pointer | Quits at once |
| Quit in the Dock icon's menu, log-out, another app asking | Quits at once |
| `Cmd+Q` | Quits on the second press |

The first `Cmd+Q` shows "Press ⌘Q again to quit" in the middle of the screen
for 1.5 seconds. A second press within that time quits; a later one counts
as a first press again. Holding the keys down does not count as a second
press. The hint shows above every window, full-screen ones included, and
never takes a click. It works the same with no window open.

`Cmd+Q` is one of the keys that cannot be given to a shortcut
(`shortcuts.md`).

## Starting again

Each window opens where it was when Magnetite quit: the same size and
place, and maximized if it was. The windows besides the first open again
only while several windows are on (`spaces.md`).

- A window that would not be on a screen now (a screen that is gone, a
  smaller one) moves onto the screen it overlaps most, or the nearest one,
  and gets smaller where it does not fit.
- Full screen and minimized do not carry over: the window opens as it was
  before.
- On Wayland the system places windows itself: the size and whether the
  window is maximized carry over, the place does not.
- A window opened while Magnetite runs (`Cmd+N`) opens centred, as does
  the first window of a new installation.

## Data

Each window's place is stored in `state.json`, on this device only
(`placement` for the first window, and in each entry of `windows`): its
size and place while not maximized, and whether it is maximized. It is read
0.3 seconds after the window last moved or changed size, so the frames of
an animation never count.

Whether the last window is closed, and the time of the first `Cmd+Q`, live
in memory only.

Code: `crates/engine/src/frame.rs` (`can_close`, `hide_frame`,
`show_frame`, `create_frame`, `place_frame`), `placement.rs`,
`mac/mod.rs` (`terminate:`, the Dock's reopen),
`mac/quit.rs` (the second press and its hint), `menubar.rs` (`runCommand:`),
`crates/app/src/app/events.rs` (`FrameHidden`, `FramePlaced`),
`crates/app/src/state/windows.rs`.

## Not built yet

- Keeping Magnetite running without a window on Linux and Windows.
- A setting to quit with the first `Cmd+Q`.
- A "New Window" entry in the Dock icon's menu.
