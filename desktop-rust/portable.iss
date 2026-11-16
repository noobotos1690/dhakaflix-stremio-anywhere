; DhakaFlix Relay - single-file portable build.
; Unpacks the whole payload (dashboard, Node runtime, npm packages, cloudflared)
; into a folder you choose. No shortcuts, no registry entries, no uninstaller:
; to remove it, turn off "Launch at Windows startup" if you enabled it, quit from
; the tray, then delete the folder.

#define AppName "DhakaFlix Relay"
#define AppVersion "1.3.1"
#define AppExe "DhakaFlixRelay.exe"
#define AppPublisher "DhakaFlix"

[Setup]
AppId={{C4E1A9F7-58B2-4D3A-9E6C-1B7F2A4D6C08}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion} (portable)
AppPublisher={#AppPublisher}
DefaultDirName={localappdata}\Programs\DhakaFlix Relay
DisableProgramGroupPage=yes
DisableWelcomePage=no
DisableDirPage=no
PrivilegesRequired=lowest
OutputDir=dist
OutputBaseFilename=DhakaFlixRelay-Portable-{#AppVersion}
SetupIconFile=assets\app.ico
Uninstallable=no
CreateUninstallRegKey=no
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
RestartApplications=no
VersionInfoDescription=DhakaFlix Relay portable
VersionInfoProductName={#AppName}
VersionInfoProductVersion={#AppVersion}
VersionInfoCompany={#AppPublisher}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "dist\app\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch DhakaFlix Relay"; Flags: nowait postinstall skipifsilent
