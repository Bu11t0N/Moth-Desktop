Target x86-unicode
Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"
!ifndef VERSION
  !error "VERSION is required"
!endif
!ifndef PAYLOAD
  !error "PAYLOAD is required"
!endif
!ifndef OUTPUT
  !error "OUTPUT is required"
!endif
Name "Moth Desktop ${VERSION}"
OutFile "${OUTPUT}"
RequestExecutionLevel admin
InstallDir "$PROGRAMFILES64\Moth Desktop"
SetCompressor /SOLID lzma
VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "Moth Desktop"
VIAddVersionKey /LANG=1033 "FileDescription" "Moth Desktop installer"
VIAddVersionKey /LANG=1033 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "GPL-3.0-or-later"
ShowInstDetails show
ShowUninstDetails show
!define MUI_ABORTWARNING
!define MUI_WELCOMEPAGE_TEXT "Установка приложения, системной службы и проверенных ядер VPN.$\r$\n$\r$\nПеред обновлением отключите VPN и выберите «Выход» в меню трея. Профили сохранятся в учётной записи Windows.$\r$\n$\r$\nСборка для разработки без Authenticode-подписи."
!define MUI_FINISHPAGE_TEXT "Приложение установлено. Откройте Moth Desktop через ярлык в меню «Пуск».$\r$\n$\r$\nДля обычной работы права администратора не нужны."
!insertmacro MUI_PAGE_WELCOME
!ifndef LICENSE_FILE
  !error "LICENSE_FILE is required (BOM-marked Unicode license)"
!endif
!insertmacro MUI_PAGE_LICENSE "${LICENSE_FILE}"
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!define MUI_UNCONFIRMPAGE_TEXT_TOP "Приложение и служба будут удалены. Профили, настройки, изменённые файлы и резервные копии сохранятся. Сначала выберите «Выход» в трее."
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Russian"
Var PackageDir
Var Result

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "Требуется Windows x64."
    Abort
  ${EndIf}
  SetRegView 64
  SetShellVarContext all
  ; /D cannot redirect privileged installation to a caller-selected path.
  StrCpy $INSTDIR "$PROGRAMFILES64\Moth Desktop"
FunctionEnd

Section "Moth Desktop" SEC_MAIN
  ; The payload is extracted under the protected Program Files parent, never a
  ; user-writable TEMP directory. The service transaction owns promotion/rollback.
  GetTempFileName $PackageDir "$PROGRAMFILES64"
  IfErrors failed
  Delete "$PackageDir"
  StrCpy $PackageDir "$PackageDir.moth-payload"
  ; Set owner and DACL AT creation: an elevated user's SID must not become
  ; owner of executable staging that its unelevated token could later rewrite.
  System::Call 'advapi32::ConvertStringSecurityDescriptorToSecurityDescriptorW(w "O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)", i 1, *p .r0, p 0) i .r1'
  StrCmp $1 0 failed
  System::Call '*(i 12, p r0, i 0) p .r2'
  System::Call 'kernel32::CreateDirectoryW(w "$PackageDir", p r2) i .r1'
  System::Free $2
  System::Call 'kernel32::LocalFree(p r0)'
  StrCmp $1 0 failed
  SetOutPath "$PackageDir"
  File /r "${PAYLOAD}\*"
  WriteUninstaller "$PackageDir\Uninstall.exe"
  IfErrors failed
  nsExec::ExecToLog '"$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PackageDir\Service.ps1" -Action Install -PackageDirectory "$PackageDir" -InstallDesktop'
  Pop $Result
  ${If} $Result != 0
    Goto failed
  ${EndIf}
  SetOutPath "$INSTDIR"
  CreateShortcut "$SMPROGRAMS\Moth Desktop.lnk" "$INSTDIR\moth-desktop.exe"
  CreateShortcut "$DESKTOP\Moth Desktop.lnk" "$INSTDIR\moth-desktop.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MothDesktop" "DisplayName" "Moth Desktop"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MothDesktop" "DisplayVersion" "${VERSION}"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MothDesktop" "DisplayIcon" "$INSTDIR\moth-desktop.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MothDesktop" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MothDesktop" "UninstallString" '$\"$INSTDIR\Uninstall.exe$\"'
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MothDesktop" "QuietUninstallString" '$\"$INSTDIR\Uninstall.exe$\" /S'
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MothDesktop" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MothDesktop" "NoRepair" 1
  IfErrors failed
  ; Only this freshly created protected payload directory may be removed.
  RMDir /r "$PackageDir"
  Goto done
failed:
  SetOutPath "$PROGRAMFILES64"
  ; Keep failed staging for diagnosis; don't remove existing installation data.
  SetErrorLevel 1
  DetailPrint "Установка не завершена. Проверьте сообщения выше."
  IfSilent +2
    MessageBox MB_OK|MB_ICONSTOP "Установка не завершена. Файлы прежней установки сохранены. Подробности — в журнале установки."
  Abort
done:
SectionEnd

Function un.onInit
  SetRegView 64
  SetShellVarContext all
  StrCpy $INSTDIR "$PROGRAMFILES64\Moth Desktop"
FunctionEnd

Section "Uninstall"
  SetOutPath "$PROGRAMFILES64"
  nsExec::ExecToLog '"$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$INSTDIR\Service.ps1" -Action Remove -InstallDesktop'
  Pop $Result
  ${If} $Result != 0
    SetErrorLevel 1
    IfSilent +2
      MessageBox MB_OK|MB_ICONSTOP "Удаление не завершено. Проверьте сообщения выше."
    Abort
  ${EndIf}
  Delete "$SMPROGRAMS\Moth Desktop.lnk"
  Delete "$DESKTOP\Moth Desktop.lnk"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\MothDesktop"
SectionEnd
