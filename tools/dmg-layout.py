#!/usr/bin/env python3
"""Write packaging/macos/dmg.DS_Store: how Finder shows the macOS disk image.

    python3 -m venv /tmp/dmg-venv && /tmp/dmg-venv/bin/pip install ds_store
    /tmp/dmg-venv/bin/python tools/dmg-layout.py

The disk image (`make_dmg` in scripts/lib.sh) holds Magnetite.app and a link
to /Applications. Finder reads the window's size, the icon size and where the
two icons sit from a `.DS_Store` file in the image. That file is made here,
once, and kept in the repository, so a release needs neither Finder nor this
package. Run it again only to change the layout.
"""

from pathlib import Path

from ds_store import DSStore

OUT = Path(__file__).resolve().parent.parent / "packaging" / "macos" / "dmg.DS_Store"

# The window's place on screen and the size of its content (points).
WINDOW = "{{200, 120}, {640, 400}}"
ICON_SIZE = 128.0
# Centres of the two icons: the app on the left, Applications on the right.
ICONS = {"Magnetite.app": (170, 180), "Applications": (470, 180)}

# A bare window: no sidebar, toolbar or bars around the two icons.
window = {
    "WindowBounds": WINDOW,
    "ShowSidebar": False,
    "ContainerShowSidebar": False,
    "ShowToolbar": False,
    "ShowTabView": False,
    "ShowPathbar": False,
    "ShowStatusBar": False,
    "PreviewPaneVisibility": False,
    "SidebarWidth": 180,
}
# Icons where they are put (no grid), names below, Finder's own background.
icon_view = {
    "viewOptionsVersion": 1,
    "backgroundType": 0,
    "backgroundColorRed": 1.0,
    "backgroundColorGreen": 1.0,
    "backgroundColorBlue": 1.0,
    "arrangeBy": "none",
    "gridOffsetX": 0.0,
    "gridOffsetY": 0.0,
    "gridSpacing": 100.0,
    "iconSize": ICON_SIZE,
    "textSize": 13.0,
    "labelOnBottom": True,
    "showIconPreview": False,
    "showItemInfo": False,
    "scrollPositionX": 0.0,
    "scrollPositionY": 0.0,
}

OUT.unlink(missing_ok=True)
with DSStore.open(str(OUT), "w+") as store:
    store["."]["vSrn"] = ("long", 1)
    store["."]["bwsp"] = window
    store["."]["icvp"] = icon_view
    store["."]["icvl"] = ("type", b"icnv")
    for name, at in ICONS.items():
        store[name]["Iloc"] = at
print(f"wrote {OUT}")
