#ifndef AppVersion
  #error Pass /DAppVersion from build-installer.ps1
#endif

[Setup]
; Keep this ID for every release: updates replace the existing installation.
AppId={{2A701653-C73D-4887-A903-07A7295AB5EF}
AppName=Lernzeit
AppVersion={#AppVersion}
AppPublisher=Nikita Berger
AppPublisherURL=https://github.com/Nikityyy/lernzeit
AppSupportURL=https://github.com/Nikityyy/lernzeit/issues
AppUpdatesURL=https://github.com/Nikityyy/lernzeit/releases/latest
DefaultDirName={localappdata}\Programs\Lernzeit
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
WizardStyle=modern dynamic
SetupIconFile=lernzeit.ico
UninstallDisplayIcon={app}\Lernzeit.exe
DisableWelcomePage=yes
DisableDirPage=no
DisableProgramGroupPage=yes
DisableReadyPage=yes
UsePreviousTasks=yes
UsePreviousAppDir=yes
ShowLanguageDialog=no
AppMutex=Local\Lernzeit
SetupMutex=Local\LernzeitSetup
CloseApplications=no
RestartApplications=no
Uninstallable=yes
Compression=lzma2
SolidCompression=yes
OutputDir=..\target\installer
OutputBaseFilename=Lernzeit-Setup-{#AppVersion}
VersionInfoVersion={#AppVersion}
VersionInfoDescription=Lernzeit installieren

[Languages]
Name: "german"; MessagesFile: "compiler:Languages\German.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Messages]
german.SelectDirDesc=Wo soll Lernzeit installiert werden?
german.SelectDirLabel3=Lernzeit wird nur für dein Windows-Konto installiert. Administratorrechte sind nicht nötig.
german.SelectTasksDesc=Verknüpfungen
german.SelectTasksLabel2=Im Startmenü findest du Lernzeit nach der Installation. Möchtest du auch eine Verknüpfung auf dem Desktop?
german.SetupAppRunningError=Lernzeit läuft noch, möglicherweise im Infobereich neben der Uhr.%n%nWähle dort mit der rechten Maustaste auf das Lernzeit-Symbol „Beenden“. Deine Sitzung wird dabei gespeichert.%n%nKlicke danach auf OK, um fortzufahren, oder auf Abbrechen.
german.UninstallAppRunningError=Lernzeit läuft noch, möglicherweise im Infobereich neben der Uhr.%n%nWähle dort mit der rechten Maustaste auf das Lernzeit-Symbol „Beenden“. Klicke danach auf OK, um fortzufahren, oder auf Abbrechen.
german.FinishedLabel=Lernzeit ist installiert.%n%nDu findest die App im Startmenü. Deine Lernzeiten und Einstellungen bleiben bei Updates und beim Deinstallieren erhalten.
german.ConfirmUninstall=Möchtest du Lernzeit entfernen?%n%nDeine Lernzeiten und Einstellungen bleiben erhalten.
english.SelectDirLabel3=Lernzeit installs for your Windows account only. No administrator access is needed.
english.SelectTasksDesc=Shortcuts
english.SelectTasksLabel2=Lernzeit will be available in the Start menu. Would you also like a desktop shortcut?
english.SetupAppRunningError=Lernzeit is still running, possibly in the tray beside the clock.%n%nRight-click its tray icon and choose Beenden (Quit). This saves your session.%n%nThen click OK to continue, or Cancel to exit.
english.UninstallAppRunningError=Lernzeit is still running, possibly in the tray beside the clock.%n%nRight-click its tray icon and choose Beenden (Quit), then click OK to continue, or Cancel to exit.
english.FinishedLabel=Lernzeit is installed.%n%nOpen it from the Start menu. Your study history and settings are kept when you update or uninstall.
english.ConfirmUninstall=Remove Lernzeit?%n%nYour study history and settings will be kept.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"

[InstallDelete]
Type: files; Name: "{userdesktop}\Lernzeit.lnk"; Tasks: not desktopicon

[Files]
Source: "..\target\release\lernzeit.exe"; DestDir: "{app}"; DestName: "Lernzeit.exe"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Lernzeit"; Filename: "{app}\Lernzeit.exe"; WorkingDir: "{app}"
Name: "{userdesktop}\Lernzeit"; Filename: "{app}\Lernzeit.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\Lernzeit.exe"; Description: "{cm:LaunchProgram,Lernzeit}"; Flags: nowait postinstall skipifsilent

[Code]
procedure CurStepChanged(CurStep: TSetupStep);
begin
  { Preserve an enabled startup setting when moving from the portable app. }
  if (CurStep = ssPostInstall) and RegValueExists(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Lernzeit') then
    RegWriteStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Lernzeit', '"' + ExpandConstant('{app}\Lernzeit.exe') + '" --hidden');
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command: String;
begin
  if CurUninstallStep = usUninstall then
  begin
    { Remove only this installation's optional startup entry. Keep all study data. }
    if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Lernzeit', Command) then
      if CompareText(Command, '"' + ExpandConstant('{app}\Lernzeit.exe') + '" --hidden') = 0 then
        RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Lernzeit');
  end;
end;
