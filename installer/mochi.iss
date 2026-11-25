; Mochi installer (Inno Setup 6). Build with tools/build-installer.ps1.
; Per-user install, no admin rights required.

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef PublishDir
  #define PublishDir "..\publish"
#endif

[Setup]
AppId={{A4E7C912-3B5D-4F8A-9E2C-1D6B8A4F7E30}
AppName=Mochi
AppVersion={#AppVersion}
AppVerName=Mochi {#AppVersion}
AppPublisher=FahmiBahtiar
VersionInfoProductName=Mochi
VersionInfoProductVersion={#AppVersion}
VersionInfoProductTextVersion={#AppVersion}
VersionInfoCompany=FahmiBahtiar
VersionInfoDescription=Mochi Setup
VersionInfoVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\Mochi
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=Mochi-Setup-{#AppVersion}
SetupIconFile=..\crates\mochi-ui\assets\mochi.ico
UninstallDisplayIcon={app}\Mochi.exe
UninstallDisplayName=Mochi
LicenseFile=..\LICENSE
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
WizardImageFile=wizard-image-100.png,wizard-image-125.png,wizard-image-150.png,wizard-image-175.png,wizard-image-200.png
WizardSmallImageFile=wizard-small-100.png,wizard-small-125.png,wizard-small-150.png,wizard-small-175.png,wizard-small-200.png
WizardImageStretch=no
WizardImageBackColor=black
WizardSmallImageBackColor=black
DisableWelcomePage=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
CloseApplications=force
RestartApplications=no
ChangesEnvironment=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
english.AutoStart=Start Mochi with Windows
english.AddToPath=Add mochi-cli to PATH (so scripts and AI agents can just call "mochi-cli")

[Tasks]
Name: "autostart"; Description: "{cm:AutoStart}"
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; Flags: unchecked
Name: "addtopath"; Description: "{cm:AddToPath}"; Flags: unchecked

[Files]
Source: "{#PublishDir}\Mochi.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#PublishDir}\mochi-cli.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Mochi"; Filename: "{app}\Mochi.exe"
Name: "{autodesktop}\Mochi"; Filename: "{app}\Mochi.exe"; Tasks: desktopicon

[Registry]
; Same value format the app's own "Start with Windows" switch writes, so the two stay in sync.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Mochi"; ValueData: """{app}\Mochi.exe"" --autostart"; Tasks: autostart
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueName: "Mochi"; Flags: deletevalue; Tasks: not autostart
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "Mochi"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\Mochi.exe"; Description: "{cm:LaunchProgram,Mochi}"; Flags: nowait postinstall skipifsilent
; Silent update (/SILENT /UPDATE=1): reopen Mochi when done.
Filename: "{app}\Mochi.exe"; Flags: nowait; Check: IsUpdate

[UninstallRun]
; Remove Mochi's hooks from Claude Code, Cursor and Codex before stopping Mochi.
Filename: "{app}\Mochi.exe"; Parameters: "--remove-agent-hooks"; Flags: runhidden waituntilterminated; RunOnceId: "RemoveAgentHooks"
Filename: "{sys}\taskkill.exe"; Parameters: "/IM Mochi.exe /F"; Flags: runhidden; RunOnceId: "StopMochi"

; Note: %LOCALAPPDATA%\Mochi (tasks, water log, shelf, lyrics cache) is kept on uninstall on purpose.

[Code]
function IsUpdate: Boolean;
begin
  Result := ExpandConstant('{param:UPDATE|0}') = '1';
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  Result := '';
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/IM Mochi.exe /F', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
end;

function PathWithout(Path, Dir: String): String;
var
  Rest, Part: String;
  P: Integer;
begin
  Result := '';
  Rest := Path;
  while Rest <> '' do
  begin
    P := Pos(';', Rest);
    if P = 0 then
    begin
      Part := Rest;
      Rest := '';
    end
    else
    begin
      Part := Copy(Rest, 1, P - 1);
      Delete(Rest, 1, P);
    end;
    if (Part <> '') and (CompareText(RemoveBackslashUnlessRoot(Part), RemoveBackslashUnlessRoot(Dir)) <> 0) then
    begin
      if Result <> '' then
        Result := Result + ';';
      Result := Result + Part;
    end;
  end;
end;

procedure SetUserPath(Add: Boolean);
var
  Path, NewPath: String;
begin
  if not RegQueryStringValue(HKCU, 'Environment', 'Path', Path) then
    Path := '';
  NewPath := PathWithout(Path, ExpandConstant('{app}'));
  if Add then
  begin
    if NewPath <> '' then
      NewPath := NewPath + ';';
    NewPath := NewPath + ExpandConstant('{app}');
  end;
  if NewPath <> Path then
    RegWriteExpandStringValue(HKCU, 'Environment', 'Path', NewPath);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and (not IsUpdate) then
    SetUserPath(WizardIsTaskSelected('addtopath'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    SetUserPath(False);
end;
