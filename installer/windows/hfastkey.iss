; Bộ cài HFastKey cho Windows (Inno Setup 6). Build: iscc /DAppVersion=0.1.0 installer\windows\hfastkey.iss
#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif

[Setup]
AppId={{6F1C2B7A-3E4D-4B8A-9C21-7D5E8F0A1B32}
AppName=HFastKey
AppVersion={#AppVersion}
AppVerName=HFastKey {#AppVersion}
AppPublisher=hiep265
AppPublisherURL=https://github.com/hiep265/hfastkey
AppSupportURL=https://github.com/hiep265/hfastkey/issues
DefaultDirName={localappdata}\Programs\HFastKey
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=hfastkey-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName=HFastKey - bộ gõ tiếng Việt
UninstallDisplayIcon={app}\hfastkey.exe
CloseApplications=force

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "..\..\target\release\hfastkey.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\HFastKey"; Filename: "{app}\hfastkey.exe"; Comment: "Bộ gõ tiếng Việt"
Name: "{autoprograms}\Gỡ HFastKey"; Filename: "{uninstallexe}"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "HFastKey"; ValueData: """{app}\hfastkey.exe"""; Flags: uninsdeletevalue

[Run]
Filename: "{app}\hfastkey.exe"; Description: "Chạy HFastKey ngay"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM hfastkey.exe"; Flags: runhidden; RunOnceId: "StopHFastKey"

[UninstallDelete]
Type: filesandordirs; Name: "{userappdata}\hfastkey"
