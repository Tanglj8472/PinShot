#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef PayloadDir
  #error PayloadDir is required
#endif
#ifndef DistDir
  #error DistDir is required
#endif
#ifndef InstallerAppId
  #define InstallerAppId "{{D7F304E1-DA08-4C84-B224-3110EBD4A023}"
#endif

[Setup]
AppId={#InstallerAppId}
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
; Keep the shutdown helper and its DLLs first for solid-compressed extraction.
; Always run the NEW helper from {tmp}, never the executable being replaced.
Source: "{#PayloadDir}\pinshot.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\*.dll"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PayloadDir}\*"; DestDir: "{app}"; Excludes: "pinshot.exe,*.dll"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\PinShot"; Filename: "{app}\pinshot.exe"; IconFilename: "{app}\pinshot.ico"
Name: "{autodesktop}\PinShot"; Filename: "{app}\pinshot.exe"; IconFilename: "{app}\pinshot.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\pinshot.exe"; Description: "Launch PinShot"; Flags: nowait postinstall skipifsilent
Filename: "{app}\pinshot.exe"; Flags: nowait skipifnotsilent runasoriginaluser; Check: RestartAfterUpdate

[Code]
function RestartAfterUpdate: Boolean;
begin
  Result := ExpandConstant('{param:PINSHOTRESTART|0}') = '1';
end;

function ShutdownError(const ExePath, HelperDir: String): String;
var
  ResultCode: Integer;
begin
  Result := '';
  Log('Running shutdown helper: ' + ExePath);
  if not Exec(ExePath, '--quit-for-install "' + ExpandConstant('{app}') + '"',
    HelperDir, SW_HIDE, ewWaitUntilTerminated, ResultCode) then
    Result := 'Unable to start the PinShot shutdown helper. Windows error: ' + IntToStr(ResultCode)
  else if ResultCode <> 0 then
    Result := 'PinShot could not be closed (helper exit code ' + IntToStr(ResultCode) +
      '). Close PinShot or check its permissions, then retry.';
  if Result <> '' then Log(Result);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ExePath: String;
  HelperDir: String;
begin
  Result := '';
  try
    ExtractTemporaryFiles('{app}\pinshot.exe');
    ExtractTemporaryFiles('{app}\*.dll');
    // ExtractTemporaryFiles preserves the literal app placeholder under tmp.
    HelperDir := ExpandConstant('{tmp}') + '\{app}';
    ExePath := HelperDir + '\pinshot.exe';
    Result := ShutdownError(ExePath, HelperDir);
  except
    Result := 'Unable to prepare the PinShot shutdown helper: ' + GetExceptionMessage;
    Log(Result);
  end;
end;

function InitializeUninstall: Boolean;
var
  ErrorText: String;
  ExePath: String;
begin
  Result := True;
  ExePath := ExpandConstant('{app}\pinshot.exe');
  if not FileExists(ExePath) then Exit;
  try
    ErrorText := ShutdownError(ExePath, ExpandConstant('{app}'));
  except
    ErrorText := 'Unable to prepare PinShot for uninstall: ' + GetExceptionMessage;
    Log(ErrorText);
  end;
  if ErrorText <> '' then begin
    SuppressibleMsgBox(ErrorText, mbError, MB_OK, IDOK);
    Result := False;
  end;
end;
