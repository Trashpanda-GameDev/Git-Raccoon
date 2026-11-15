; The Windows installer that asks where to install Git Raccoon.
;
; It's a front end for Velopack's own setup (GitRaccoon-win-Setup.exe, made
; by `vpk pack`), which it runs with the folder chosen here. So the app is
; installed exactly as Velopack installs it, in that folder: it still
; updates itself, and is uninstalled from Settings > Apps as usual.
;
; Built by the Release workflow with Inno Setup (https://jrsoftware.org):
;     set VERSION=0.5.1
;     iscc installer\windows.iss
; with Velopack's setup in releases\.

#define Version GetEnv("VERSION")

[Setup]
AppId=GitRaccoon
AppName=Git Raccoon
AppVersion={#Version}
AppPublisher=Trashpanda-GameDev
AppPublisherURL=https://github.com/Trashpanda-GameDev/Git-Raccoon
DefaultDirName={localappdata}\GitRaccoon
DisableDirPage=no
DisableProgramGroupPage=yes
DisableReadyPage=no
UsePreviousAppDir=no
DirExistsWarning=no
; Installs for this user, without administrator rights: Git Raccoon updates
; itself, so it needs a folder this user can write to.
PrivilegesRequired=lowest
; Velopack's setup registers the uninstaller; this only runs it.
Uninstallable=no
CreateUninstallRegKey=no
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
SetupIconFile=..\crates\git-raccoon\assets\icon.ico
OutputDir=..\installer-out
OutputBaseFilename=GitRaccoon-win-Installer
Compression=none

[Files]
; Already compressed: it's a self-extracting package.
Source: "..\releases\GitRaccoon-win-Setup.exe"; DestDir: "{tmp}"; Flags: deleteafterinstall

[Messages]
SelectDirDesc=Where should Git Raccoon be installed?
FinishedLabelNoIcons=Git Raccoon is installed, and starting.

[Code]
// Git Raccoon updates itself in place, so the folder must be writable
// without administrator rights (Program Files isn't).
function NextButtonClick(CurPageID: Integer): Boolean;
var
  Dir, Probe: String;
begin
  Result := True;
  if CurPageID = wpSelectDir then
  begin
    Dir := WizardDirValue;
    Probe := AddBackslash(Dir) + '.git-raccoon-write-test';
    if not ForceDirectories(Dir) or not SaveStringToFile(Probe, '', False) then
    begin
      MsgBox('Git Raccoon can''t be installed in "' + Dir + '": you can''t write to that folder without administrator rights.'
        + #13#10#13#10 + 'Git Raccoon keeps itself up to date, so choose a folder you can write to (for example one in your user folder, or on another drive).',
        mbError, MB_OK);
      Result := False;
    end
    else
      DeleteFile(Probe);
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  ResultCode: Integer;
begin
  if CurStep = ssPostInstall then
  begin
    WizardForm.StatusLabel.Caption := 'Installing Git Raccoon...';
    if not Exec(ExpandConstant('{tmp}\GitRaccoon-win-Setup.exe'),
      '--silent --installto "' + ExpandConstant('{app}') + '"',
      '', SW_SHOW, ewWaitUntilTerminated, ResultCode) or (ResultCode <> 0) then
      MsgBox('Git Raccoon couldn''t be installed (error ' + IntToStr(ResultCode) + ').'
        + #13#10#13#10 + 'Try GitRaccoon-win-Setup.exe from the releases page instead, which installs to the usual folder.',
        mbError, MB_OK);
  end;
end;
