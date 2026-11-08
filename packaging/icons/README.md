# App icon

`magnetite.svg` is the source of the app icon (a flat iron ring wound with
copper wire); `magnetite-small.svg` is the simplified drawing for small sizes.
The rendered files live where each platform needs them:

- macOS: `crates/app/resources/mac/magnetite.icns` (bundle icon)
- Linux: `packaging/linux/magnetite.png` (512 px, next to the `.desktop` file)
- Windows: `packaging/windows/magnetite.ico` (shipped next to `magnetite.exe`;
  embedding it in the executable comes with the Windows bring-up)
- Sidebar UI: `crates/ui/web/src/assets/icon.png` (Settings → About, 128 px)
