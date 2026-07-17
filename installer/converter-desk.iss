; Inno Setup script for Converter Desk.
; Produces a single self-contained Windows installer that bundles the app
; together with yt-dlp and ffmpeg/ffprobe, so the end user installs everything
; at once. All binaries land in the same install directory, which lets the app
; resolve yt-dlp/ffmpeg "next to the executable" (see resolve_binary_path) and
; pass --ffmpeg-location explicitly.
;
; Build with:  ISCC installer\converter-desk.iss
; (run from the repository root, or point ISCC at this file)

#define MyAppName "Converter Desk"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "Daryhen"
#define MyAppExeName "converter-desk.exe"

[Setup]
; A stable, unique AppId so upgrades/uninstalls are tracked correctly.
AppId={{7C1E9A34-2B5D-4F81-9E6A-3D8C5F2A1B90}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
; Installer output — a single setup executable.
OutputDir=output
OutputBaseFilename=converter-desk-setup
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; Install into the real 64-bit Program Files and require admin to write there.
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
; The application itself (release build).
Source: "..\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; Bundled external tools — installed alongside the app so no separate install is needed.
Source: "assets\yt-dlp.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "assets\ffmpeg.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "assets\ffprobe.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#MyAppName}}"; Flags: nowait postinstall skipifsilent
