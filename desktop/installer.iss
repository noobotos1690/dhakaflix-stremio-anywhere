; DhakaFlix Relay - per-user installer (no administrator rights required).
; The app itself requests elevation only for the optional system-wide Node.js
; install. Built with Inno Setup 6.

#define AppName "DhakaFlix Relay"
#define AppVersion "1.2.0"
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
WizardImageFile=assets\wizard.bmp
WizardSmallImageFile=assets\wizard-small.png
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

[Code]
function HasDotNet48(): Boolean;
var
  release: Cardinal;
begin
  Result := RegQueryDWordValue(HKLM, 'SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full', 'Release', release)
            and (release >= 528040);
  if not Result then
    Result := RegQueryDWordValue(HKLM32, 'SOFTWARE\Microsoft\NET Framework Setup\NDP\v4\Full', 'Release', release)
              and (release >= 528040);
end;

function InitializeSetup(): Boolean;
begin
  Result := True;

  if HasDotNet48() then
    exit;

  if WizardSilent then
    exit;

  if MsgBox('Microsoft .NET Framework 4.8 was not detected.' + #13#10 + #13#10 +
            'DhakaFlix Relay needs it to run. Windows 10 (May 2019 update or newer) and ' +
            'Windows 11 include it. Continue anyway?', mbError, MB_YESNO) = IDNO then
    Result := False;
end;
