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
AppSupportURL=https://github.com/Tanglj8472/pinshot/issues
AppUpdatesURL=https://github.com/Tanglj8472/pinshot/releases
VersionInfoCompany=PinShot Project
VersionInfoDescription=PinShot screen capture and annotation tool
VersionInfoProductName=PinShot
VersionInfoProductVersion={#AppVersion}
VersionInfoTextVersion={#AppVersion}
VersionInfoOriginalFileName=PinShot-{#AppVersion}-windows-x64-setup.exe
VersionInfoCopyright=Copyright (c) 2026 PinShot Project
DefaultDirName={localappdata}\Programs\PinShot
; Keep all managed installs in one predictable per-user directory.
DisableDirPage=yes
UsePreviousAppDir=no
AlwaysShowDirOnReadyPage=yes
DefaultGroupName=PinShot
PrivilegesRequired=lowest
; The linked ONNX runtime imports DXCore (Windows 10 2004 / build 19041).
MinVersion=10.0.19041
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
ShowLanguageDialog=no
Uninstallable=yes

[Languages]
Name: "chinesesimp"; MessagesFile: "ChineseSimplified.isl"

[Tasks]
Name: "desktopicon"; Description: "创建桌面快捷方式"; Flags: unchecked

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
Filename: "{app}\pinshot.exe"; Description: "启动 PinShot"; Flags: nowait postinstall skipifsilent
Filename: "{app}\pinshot.exe"; Flags: nowait skipifnotsilent runasoriginaluser; Check: RestartAfterUpdate

[Code]
function RestartAfterUpdate: Boolean;
begin
  Result := ExpandConstant('{param:PINSHOTRESTART|0}') = '1';
end;

function InstalledVersionMatches: Boolean;
var
  Major: Word;
  Minor: Word;
  Revision: Word;
  Build: Word;
  Actual: String;
begin
  // Inno Setup 6: GetVersionNumbers is Filename + VersionMS + VersionLS.
  // Four-part Major/Minor/Revision/Build requires GetVersionComponents (Word).
  Result := GetVersionComponents(ExpandConstant('{app}\pinshot.exe'), Major, Minor, Revision, Build);
  if Result then begin
    Actual := IntToStr(Major) + '.' + IntToStr(Minor) + '.' + IntToStr(Revision);
    Result := Actual = '{#AppVersion}';
  end else begin
    Actual := '<unavailable>';
  end;
  Log('Installed PinShot version check: expected={#AppVersion}, actual=' + Actual);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then begin
    if not InstalledVersionMatches then
      SuppressibleMsgBox('PinShot 安装后的版本校验失败，请重新安装。', mbError, MB_OK, IDOK);
  end;
end;

function ShutdownError(const ExePath, HelperDir: String): String;
var
  ResultCode: Integer;
  Attempt: Integer;
begin
  Result := '';
  Log('Running shutdown helper: ' + ExePath);
  for Attempt := 1 to 3 do begin
    if not Exec(ExePath, '--quit-for-install "' + ExpandConstant('{app}') + '"',
      HelperDir, SW_HIDE, ewWaitUntilTerminated, ResultCode) then begin
      Result := '无法启动 PinShot 关闭助手，Windows 错误：' + IntToStr(ResultCode);
      Break;
    end;
    if ResultCode = 0 then Exit;
    Log('Shutdown helper exit code ' + IntToStr(ResultCode) + ', attempt ' + IntToStr(Attempt));
    // A process may be between exiting and releasing its image. Retry the
    // transient failure automatically, just as going Back and Next would.
    if (ResultCode <> 1) or (Attempt = 3) then begin
      Result := '无法关闭 PinShot（helper exit code ' + IntToStr(ResultCode) +
        '）。请退出 PinShot 或检查权限后重试。';
      Break;
    end;
    Sleep(500);
  end;
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
    Result := '无法准备 PinShot 关闭助手：' + GetExceptionMessage;
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
    ErrorText := '无法准备卸载 PinShot：' + GetExceptionMessage;
    Log(ErrorText);
  end;
  if ErrorText <> '' then begin
    SuppressibleMsgBox(ErrorText, mbError, MB_OK, IDOK);
    Result := False;
  end;
end;
