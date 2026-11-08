#define AppName "DhakaFlix Relay"
#define AppVersion "2.0.0"
#define AppPublisher "DhakaFlix"
#define AppExe "DhakaFlixRelay.exe"

[Setup]
AppId={{A0B7C1D2-6F44-4C58-9A1B-2D7E5F8091A3}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher={#AppPublisher}
DefaultDirName={localappdata}\Programs\DhakaFlix Relay
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.17763
OutputDir=dist
OutputBaseFilename=DhakaFlixRelay-Setup-{#AppVersion}
SetupIconFile=assets\app.ico
WizardImageFile=assets\wizard.bmp
WizardSmallImageFile=assets\wizard-small.png
UninstallDisplayIcon={app}\{#AppExe}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
VersionInfoDescription=DhakaFlix Relay setup
VersionInfoProductName={#AppName}
VersionInfoProductVersion={#AppVersion}
VersionInfoCompany={#AppPublisher}

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked
Name: "startup"; Description: "Start DhakaFlix Relay when I sign in to Windows"; GroupDescription: "Startup:"; Flags: unchecked

[Files]
Source: "dist\app\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#AppExe}"
Name: "{group}\Uninstall {#AppName}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExe}"; Tasks: desktopicon

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
Type: filesandordirs; Name: "{app}\src"
Type: filesandordirs; Name: "{app}\scripts"
Type: files; Name: "{app}\.env"
Type: files; Name: "{app}\package-lock.json"
