# App icon

`magnetite.svg` is the source of the app icon (a flat iron ring wound with
copper wire); `magnetite-small.svg` is the simplified drawing for small sizes.
The rendered files live where each platform needs them:

- macOS: `crates/app/resources/mac/magnetite.icns` (bundle icon). It follows
  Apple's grid: the rounded square is 824 px of the 1024 px canvas, centred,
  with a soft drop shadow in the margin. Drawn edge to edge it looks bigger
  than every other icon in the Dock.
- Linux: `packaging/linux/magnetite.png` (512 px, next to the `.desktop` file)
- Windows: `packaging/windows/magnetite.ico` (shipped next to `magnetite.exe`
  for the shortcuts, and put into the executable by
  `scripts/windows/brand-exe.ps1`). Its 16 and 32 px images are the simplified
  drawing; from 48 px, the size of a desktop icon, it is the full one.
- Sidebar UI: `crates/ui/web/src/assets/icon.png` (Settings → About, 128 px)
