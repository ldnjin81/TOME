; Installer hooks (bundle.windows.nsis.installerHooks).
;
; Up to 0.5.1 the product name was "TOME", so the installer registered itself as "TOME" in
; %LOCALAPPDATA%\TOME. winget matches that entry, case-insensitively, to an unrelated app
; ("Tome", Runebook.Tome) and `winget upgrade --all` installed that app over TOME. From 0.5.2 the
; product name is "TOME VCS". After installing, the old "TOME" installation is removed with its own
; uninstaller, but only when its publisher is ours, so another app named "Tome" is never touched.
; The uninstaller runs silently, which keeps the settings in %APPDATA%\dev.ldnjin.tome.

!define LEGACY_UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\TOME"
!define LEGACY_MANUPRODUCTKEY "Software\${MANUFACTURER}\TOME"

!macro NSIS_HOOK_POSTINSTALL
  Push $R0
  Push $R1
  Push $R2
  ReadRegStr $R0 HKCU "${LEGACY_UNINSTKEY}" "Publisher"
  ${If} $R0 == "${MANUFACTURER}"
    ReadRegStr $R1 HKCU "${LEGACY_UNINSTKEY}" "InstallLocation"
    ; The installer stores the location in quotes.
    StrCpy $R2 $R1 1
    ${If} $R2 == '"'
      StrCpy $R1 $R1 "" 1
      StrCpy $R1 $R1 -1
    ${EndIf}
    ${If} $R1 != ""
    ${AndIf} $R1 != $INSTDIR
    ${AndIf} ${FileExists} "$R1\uninstall.exe"
      ; _?= runs the uninstaller in place and waits for it; it then cannot delete itself.
      ExecWait '"$R1\uninstall.exe" /S _?=$R1'
      Delete "$R1\uninstall.exe"
      RMDir "$R1"
    ${EndIf}
    ; If the old uninstaller is missing or failed, still drop the old registration.
    ReadRegStr $R0 HKCU "${LEGACY_UNINSTKEY}" "Publisher"
    ${If} $R0 == "${MANUFACTURER}"
      DeleteRegKey HKCU "${LEGACY_UNINSTKEY}"
    ${EndIf}
    DeleteRegKey HKCU "${LEGACY_MANUPRODUCTKEY}"
  ${EndIf}
  Pop $R2
  Pop $R1
  Pop $R0
!macroend
