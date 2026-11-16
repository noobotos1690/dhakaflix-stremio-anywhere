; DhakaFlix Relay (Rust + Slint) - per-user installer, no administrator rights.
; The app never elevates: Node.js is downloaded into its own folder.

#define AppName "DhakaFlix Relay"
#define AppVersion "1.3.1"
#define AppExe "DhakaFlixRelay.exe"
#define AppPublisher "DhakaFlix"

[Setup]
AppId={{A0B7C1D2-6F44-4C58-9A1B-2D7E5F8091A3}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={localappdata}\Programs\DhakaFlix Relay
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
DisableWelcomePage=no
DisableDirPage=no
PrivilegesRequired=lowest
OutputDir=dist
OutputBaseFilename=DhakaFlixRelay-Setup-{#AppVersion}
SetupIconFile=assets\app.ico
UninstallDisplayIcon={app}\{#AppExe}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
CloseApplications=yes
RestartApplications=no
AppMutex=Local\DhakaFlixRelay.Desktop
VersionInfoDescription=DhakaFlix Relay setup
VersionInfoProductName={#AppName}
VersionInfoProductVersion={#AppVersion}
VersionInfoCompany={#AppPublisher}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked
Name: "startup"; Description: "Start DhakaFlix Relay when I sign in to Windows"; GroupDescription: "Startup:"; Flags: unchecked

[Files]
Source: "dist\app\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\DhakaFlix Relay"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"
Name: "{group}\Uninstall DhakaFlix Relay"; Filename: "{uninstallexe}"
Name: "{autodesktop}\DhakaFlix Relay"; Filename: "{app}\{#AppExe}"; WorkingDir: "{app}"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; \
    ValueName: "DhakaFlix Relay"; ValueData: """{app}\{#AppExe}"" --autostart"; \
    Flags: uninsdeletevalue; Tasks: startup

[Run]
Filename: "{app}\{#AppExe}"; Description: "Launch DhakaFlix Relay"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: filesandordirs; Name: "{app}\node_modules"
Type: filesandordirs; Name: "{app}\runtime"
Type: filesandordirs; Name: "{app}\tools"
Type: filesandordirs; Name: "{app}\logs"
Type: filesandordirs; Name: "{app}\.run"
Type: files; Name: "{app}\.env"
