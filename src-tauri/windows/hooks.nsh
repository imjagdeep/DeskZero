; Installer hooks (Tauri NSIS). After installing or updating, ask Windows to
; refresh its icon cache so the taskbar, Start menu and pinned shortcuts show
; the new icon straight away instead of the cached old one.
!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$SYSDIR\ie4uinit.exe" -show'
!macroend
