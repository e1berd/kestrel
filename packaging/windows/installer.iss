[Setup]
AppId=dev.kestrel.app
AppName=Kestrel
AppVersion={#AppVersion}
AppPublisher=Kestrel contributors
AppPublisherURL=https://github.com/e1berd/kestrel
DefaultDirName={localappdata}\Programs\Kestrel
DefaultGroupName=Kestrel
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputBaseFilename=Kestrel-{#AppVersion}-x86_64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayIcon={app}\kestrel.exe
CloseApplications=yes

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\Kestrel"; Filename: "{app}\kestrel.exe"

[Run]
Filename: "{app}\kestrel.exe"; Description: "Open Kestrel"; Flags: nowait postinstall skipifsilent
