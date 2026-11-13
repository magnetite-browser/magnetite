; Inno Setup script of Magnetite's Windows installer. scripts/build.sh runs it
; when ISCC.exe (Inno Setup 6) is installed:
;   ISCC /DVersion=<version> /DSource=<package folder> /DOutput=<folder> /DArch=<x86_64|aarch64> magnetite.iss
;
; Installs for the current user, without administrator rights. The user's
; data (%LOCALAPPDATA%\Magnetite) is a different folder and stays on uninstall.

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
DefaultDirName={localappdata}\Programs\Magnetite
DefaultGroupName=Magnetite
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
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
; A running Magnetite holds its files open: the installer asks to close it.
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

[Run]
Filename: "{app}\magnetite.exe"; Description: "{cm:LaunchProgram,Magnetite}"; Flags: nowait postinstall skipifsilent
