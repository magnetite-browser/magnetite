; Inno Setup script of Magnetite's Windows installer. scripts/build.sh runs it
; when ISCC.exe (Inno Setup 6) is installed:
;   ISCC /DVersion=<version> /DSource=<package folder> /DOutput=<folder> /DArch=<x86_64|aarch64> magnetite.iss
;
; Asks whether to install for all users (the default: administrator rights,
; Program Files, as other browsers do) or for the current user only (no
; rights needed, %LOCALAPPDATA%\Programs), and then for the folder. Silent:
; /ALLUSERS or /CURRENTUSER, and /DIR=<folder>. Each user's data
; (%LOCALAPPDATA%\Magnetite) is a different folder and stays on uninstall.

#ifndef Version
  #error Pass /DVersion=<version>
#endif
#ifndef Source
  #error Pass /DSource=<package folder>
#endif
#ifndef Output
  #define Output "."
#endif
#ifndef Arch
  #define Arch "x86_64"
#endif

[Setup]
; Never change the AppId after a release: updates and uninstall find the
; installed copy by it.
AppId={{9D9BD2F8-0E88-47D0-B3A7-41972E6897A8}
AppName=Magnetite
AppVersion={#Version}
AppPublisher=StackForge UG (haftungsbeschränkt)
AppPublisherURL=https://magnetitebrowser.org
AppSupportURL=https://github.com/magnetite-browser/magnetite/issues
DefaultDirName={autopf}\Magnetite
DefaultGroupName=Magnetite
DisableProgramGroupPage=yes
; The folder is asked for at a first install; an update goes where the
; installed copy is, for the users it was installed for.
DisableDirPage=auto
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=dialog commandline
UsePreviousPrivileges=yes
LicenseFile={#Source}\LICENSE
SetupIconFile={#Source}\magnetite.ico
UninstallDisplayIcon={app}\magnetite.ico
UninstallDisplayName=Magnetite
OutputDir={#Output}
OutputBaseFilename=Magnetite-{#Version}-windows-{#Arch}-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; Chromium supports Windows 10 and later.
MinVersion=10.0
#if Arch == "aarch64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
; A running Magnetite holds only some of its files open: installing or
; uninstalling under it leaves a window without its pages and icon. Both
; ask to close it first ([Code] below), by the names it holds while it
; runs (crates/engine/src/win/running.rs).
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Source}\*"; DestDir: "{app}"; Flags: recursesubdirs ignoreversion

[Icons]
; magnetite.exe is CEF's bootstrap and carries CEF's icon: the shortcuts name ours.
Name: "{autoprograms}\Magnetite"; Filename: "{app}\magnetite.exe"; IconFilename: "{app}\magnetite.ico"
Name: "{autodesktop}\Magnetite"; Filename: "{app}\magnetite.exe"; IconFilename: "{app}\magnetite.ico"; Tasks: desktopicon

[Registry]
; Magnetite as a browser Windows can offer as the default: for all users
; (HKLM) or this one (HKCU), as it is installed (HKA).
; Windows starts it with the link after --single-argument: whatever the link
; looks like, it is then one argument and never an option.
Root: HKA; Subkey: "Software\Classes\MagnetiteURL"; ValueType: string; ValueData: "Magnetite URL"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\MagnetiteURL"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\MagnetiteURL\DefaultIcon"; ValueType: string; ValueData: "{app}\magnetite.exe,0"
; What Windows calls the program in its list of default apps.
Root: HKA; Subkey: "Software\Classes\MagnetiteURL\Application"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Magnetite"
Root: HKA; Subkey: "Software\Classes\MagnetiteURL\Application"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\magnetite.exe,0"
Root: HKA; Subkey: "Software\Classes\MagnetiteURL\Application"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "A fast, open-source browser built around a sidebar."
Root: HKA; Subkey: "Software\Classes\MagnetiteURL\shell\open\command"; ValueType: string; ValueData: """{app}\magnetite.exe"" --single-argument %1"
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\Magnetite"; ValueType: string; ValueData: "Magnetite"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\Magnetite\DefaultIcon"; ValueType: string; ValueData: "{app}\magnetite.exe,0"
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\Magnetite\shell\open\command"; ValueType: string; ValueData: """{app}\magnetite.exe"""
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\Magnetite\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Magnetite"
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\Magnetite\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "A fast, open-source browser built around a sidebar."
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\Magnetite\Capabilities"; ValueType: string; ValueName: "ApplicationIcon"; ValueData: "{app}\magnetite.exe,0"
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\Magnetite\Capabilities\URLAssociations"; ValueType: string; ValueName: "http"; ValueData: "MagnetiteURL"
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\Magnetite\Capabilities\URLAssociations"; ValueType: string; ValueName: "https"; ValueData: "MagnetiteURL"
Root: HKA; Subkey: "Software\Clients\StartMenuInternet\Magnetite\Capabilities\StartMenu"; ValueType: string; ValueName: "StartMenuInternet"; ValueData: "Magnetite"
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Magnetite"; ValueData: "Software\Clients\StartMenuInternet\Magnetite\Capabilities"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\magnetite.exe"; Description: "{cm:LaunchProgram,Magnetite}"; Flags: nowait postinstall skipifsilent
; An update started by Magnetite: back to where the user was. As the user
; who runs Magnetite, also when the installer itself runs as an
; administrator: a browser must never run with those rights.
Filename: "{app}\magnetite.exe"; Flags: nowait runasoriginaluser; Check: IsUpdate

[Code]
// Magnetite updates itself by running this installer with
//   /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /UPDATE=1
// (crates/app/src/update.rs). The installer then says that it stands ready
// (after Windows asked for an administrator, where the installed copy is
// one for all users), Magnetite quits, and the installer goes on.

const
  // The names Magnetite holds while it runs.
  RunningNames = 'MagnetiteRunning,Global\MagnetiteRunning';
  // The name Magnetite looks for before it quits for an update.
  ReadyName = 'MagnetiteUpdateReady';
  // How long an update waits for Magnetite to quit, in milliseconds.
  QuitWait = 60000;

function IsUpdate: Boolean;
begin
  Result := ExpandConstant('{param:UPDATE|0}') = '1';
end;

function InitializeSetup: Boolean;
var
  Waited: Integer;
begin
  Result := True;
  if IsUpdate then
  begin
    CreateMutex(ReadyName);
    Waited := 0;
    while CheckForMutexes(RunningNames) and (Waited < QuitWait) do
    begin
      Sleep(200);
      Waited := Waited + 200;
    end;
    if CheckForMutexes(RunningNames) then
      Result := False
    else
      // Its other processes end a moment after the window's.
      Sleep(1000);
    Exit;
  end;
  while CheckForMutexes(RunningNames) do
    if SuppressibleMsgBox(FmtMessage(SetupMessage(msgSetupAppRunningError), ['Magnetite']),
         mbError, MB_OKCANCEL, IDCANCEL) <> IDOK then
    begin
      Result := False;
      Exit;
    end;
end;

function InitializeUninstall: Boolean;
begin
  Result := True;
  while CheckForMutexes(RunningNames) do
    if SuppressibleMsgBox(FmtMessage(SetupMessage(msgUninstallAppRunningError), ['Magnetite']),
         mbError, MB_OKCANCEL, IDCANCEL) <> IDOK then
    begin
      Result := False;
      Exit;
    end;
end;
