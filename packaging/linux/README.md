# Magnetite for Linux

Magnetite is a desktop browser with sidebar tabs and spaces, built on
Chromium (CEF). This folder holds everything it needs: keep the files
together.

## Start

```sh
./magnetite
```

Your data lives in `~/.local/share/magnetite` (or `$XDG_DATA_HOME/magnetite`).

Magnetite needs the usual Chromium libraries (GTK 3, NSS, ALSA, libdrm, GBM,
X11/XKB). Most desktops have them. On Debian or Ubuntu:

```sh
sudo apt install libgtk-3-0 libnss3 libasound2 libgbm1 libxkbcommon0 libdrm2
```

(On Ubuntu 24.04 and later the ALSA package is `libasound2t64`.)

## The sandbox

Chromium isolates web pages in a sandbox. It needs one of the two options
below; without either it stops at start-up with "No usable sandbox".

1. **User namespaces** (most distributions): nothing to do. Ubuntu 23.10 and
   later restrict them through AppArmor; allow them for Magnetite with a
   profile (adjust the path to where you put this folder):

   ```sh
   sudo tee /etc/apparmor.d/magnetite >/dev/null <<'EOF'
   abi <abi/4.0>,
   include <tunables/global>
   profile magnetite /opt/magnetite/{magnetite,magnetite-helper} flags=(unconfined) {
     userns,
     include if exists <local/magnetite>
   }
   EOF
   sudo apparmor_parser -r /etc/apparmor.d/magnetite
   ```

2. **The setuid helper**: make `chrome-sandbox` owned by root with the
   setuid bit. Archives cannot carry this, so do it once after unpacking:

   ```sh
   sudo chown root:root chrome-sandbox
   sudo chmod 4755 chrome-sandbox
   ```

## Menu entry

```sh
sudo mv magnetite-* /opt/magnetite         # or any folder you like
sudo ln -s /opt/magnetite/magnetite /usr/local/bin/magnetite
cp /opt/magnetite/magnetite.desktop ~/.local/share/applications/
mkdir -p ~/.local/share/icons/hicolor/512x512/apps
cp /opt/magnetite/magnetite.png ~/.local/share/icons/hicolor/512x512/apps/
```

## Licences

Magnetite is licensed under the EUPL-1.2 (`LICENSE`). Third-party code keeps
its own licence: `THIRD_PARTY_NOTICES.md` for Magnetite's dependencies and
`CREDITS.html` for Chromium and CEF.
