#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef PayloadDir
  #error PayloadDir is required
#endif
#ifndef DistDir
  #error DistDir is required
#endif

[Setup]
AppId={{D7F304E1-DA08-4C84-B224-3110EBD4A023}
AppName=PinShot
AppVersion={#AppVersion}
AppPublisher=PinShot
AppPublisherURL=https://github.com/Tanglj8472/pinshot
DefaultDirName={localappdata}\Programs\PinShot
DefaultGroupName=PinShot
PrivilegesRequired=lowest
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#DistDir}
OutputBaseFilename=PinShot-{#AppVersion}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\assets\pinshot.ico
UninstallDisplayIcon={app}\pinshot.ico
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#PayloadDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\PinShot"; Filename: "{app}\pinshot.exe"; IconFilename: "{app}\pinshot.ico"
Name: "{autodesktop}\PinShot"; Filename: "{app}\pinshot.exe"; IconFilename: "{app}\pinshot.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\pinshot.exe"; Description: "Launch PinShot"; Flags: nowait postinstall skipifsilent
