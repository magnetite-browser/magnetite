# Passwords and autofill

Magnetite uses the password manager and the form autofill that are part of
Chromium. It adds their switches to its own Settings and opens Chromium's
lists from there. What is saved stays on the device.

## What it does

- **Passwords.** After a sign-in, Chromium asks whether to save the
  password. A saved one is offered in the sign-in form of its site the next
  time.
- **Addresses.** Names, addresses, phone numbers and email addresses are
  offered in forms that ask for them. Chromium asks before it saves one.
- **Payment cards.** Card details are offered in checkout forms. Chromium
  asks before it saves one.

The question to save and the suggestions inside a page are Chromium's own:
the question opens at the top left of the page.

Each profile has its own passwords, addresses and cards (`spaces.md`), so a
space with separate logins does not offer what another one saved.

## Settings

Settings → Passwords & autofill:

| Setting | Default |
|---|---|
| Offer to save passwords | on |
| Save and fill in addresses | on |
| Save and fill in payment cards | on |
| Warn about passwords from data breaches | off |

The switches count for every profile. They are this device's and do not
sync. Magnetite sets them at each start and whenever a setting changes.

Below each of the first three is a button to what is saved:

| Button | Opens |
|---|---|
| Saved passwords… | `chrome://password-manager/passwords`: the list, with adding, editing, removing and the import of a CSV file |
| Saved addresses… | `chrome://settings/contactInfo` |
| Saved cards… | `chrome://settings/payments` |

These are Chromium's own pages. They open as a peek over Settings
(`peek.md`), in the profile chosen under **Logins**. That choice shows when
there is more than one lasting profile and starts with the profile of the
current space; private profiles are not listed.

While Magnetite runs with `--mock-keychain`, the section says that saved
passwords are not protected by the keychain.

## What contacts a server

Nothing, unless the breach warning is turned on. With it, Chromium asks
Google's service (`passwordsleakcheck-pa.googleapis.com`) after a sign-in
whether the password is known from a breach; it sends an encrypted fragment,
not the password.

Chromium's lookup of form fields at Google's autofill server is switched off
(`docs/design/security.md`, "Network"): its local rules decide what a field
is for.

## Data

- `<data-dir>/cef/<profile>/Login Data` holds the passwords, encrypted with
  a key from the macOS Keychain. `Web Data` in the same folder holds
  addresses and cards. Both are deleted with their profile.
- The four switches are `settings.autofill` in `<data-dir>/state.json`.
- None of this is part of sync (`sync.md`), and the import from another
  browser does not read passwords (`import.md`).

## Limits

- Chromium's pages have switches of their own for the same things. One
  changed there lasts until Magnetite sets its own again: at the next start
  or the next change in Settings.
- The lists of addresses and cards are part of Chromium's settings page,
  with its own navigation to settings Magnetite does not use.

## Not built yet

- Lists of Magnetite's own in Settings, in place of Chromium's pages.
- Syncing passwords, addresses or cards between devices.
- The connection between a password manager's extension and its desktop
  app (native messaging): the extensions work (`extensions.md`), unlocking
  through the desktop app and iCloud Passwords do not.

Code: `crates/engine/src/autofill.rs` (`AutofillPrefs`, `set_autofill`),
`crates/engine/src/cef_app.rs` (`NO_CALLS_HOME`),
`crates/app/src/app/commands.rs` (`OpenEnginePage`),
`crates/ui/web/src/settings/AutofillSection.tsx`.
