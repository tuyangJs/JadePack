Unicode true
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"
!include "WordFunc.nsh"
!include "nsDialogs.nsh"

!define WEBVIEW2APPGUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
!define PRODUCTNAME "{{app_name}}"
!define VERSION "{{app_version}}"
!define MAINBINARYNAME "{{main_exe}}"
!define APP_ID "{{app_id}}"
{{compression_directive}}
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCTNAME}"
!define MANUPRODUCTKEY "Software\${PRODUCTNAME}"

; --- CheckIfAppIsRunning 宏（参考 Tauri nsis_tauri_utils） ---
!macro CheckIfAppIsRunning executableName productName
  !define UniqueID ${__LINE__}
  check_running_${UniqueID}:
  nsis_tauri_utils::FindProcess "${executableName}"
  Pop $R0
  ${If} $R0 = 0
    MessageBox MB_OKCANCEL|MB_ICONQUESTION "${productName} 正在运行，是否结束进程并继续？" IDOK kill_${UniqueID}
    Abort "${productName} 正在运行，请关闭后重试"
    kill_${UniqueID}:
    nsis_tauri_utils::KillProcess "${executableName}"
    Pop $R0
    Sleep 500
    ${If} $R0 = 0
    ${OrIf} $R0 = 2
      Goto done_${UniqueID}
    ${Else}
      Abort "无法结束 ${productName} 进程"
    ${EndIf}
  ${EndIf}
  done_${UniqueID}:
  !undef UniqueID
!macroend

{{scope_includes}}

Name "${PRODUCTNAME}"
BrandingText "${PRODUCTNAME}"
OutFile "{{output_exe}}"
!define PLACEHOLDER_INSTALL_DIR "placeholder\${PRODUCTNAME}"
{{install_dir_line}}
RequestExecutionLevel {{request_level}}

VIProductVersion "{{app_version}}.0"
VIAddVersionKey "ProductName" "${PRODUCTNAME}"
VIAddVersionKey "FileDescription" "${PRODUCTNAME}"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "ProductVersion" "${VERSION}"

{{icon_define}}

!define MUI_LANGDLL_REGISTRY_ROOT "HKCU"
!define MUI_LANGDLL_REGISTRY_KEY "${MANUPRODUCTKEY}"
!define MUI_LANGDLL_REGISTRY_VALUENAME "Installer Language"

; --- Variables (like Tauri) ---
Var PassiveMode
Var UpdateMode
Var NoShortcutMode

; 1. Welcome Page
!insertmacro MUI_PAGE_WELCOME

; 2. Install mode (if both)
{{install_mode_page}}

; 3. Custom page to ask user if he wants to reinstall/uninstall
Var ReinstallPageCheck
Page custom PageReinstall PageLeaveReinstall

; 4. Choose install directory page
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_DIRECTORY

; 5. Start menu shortcut page
Var AppStartMenuFolder
!define MUI_STARTMENUPAGE_DEFAULTFOLDER "${PRODUCTNAME}"
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_STARTMENU Application $AppStartMenuFolder

; 6. Installation page
!insertmacro MUI_PAGE_INSTFILES

; 7. Finish page
!define MUI_FINISHPAGE_NOAUTOCLOSE
!define MUI_FINISHPAGE_RUN
!define MUI_FINISHPAGE_RUN_TEXT "运行 ${PRODUCTNAME}"
!define MUI_FINISHPAGE_RUN_FUNCTION RunMainBinary
!define MUI_FINISHPAGE_SHOWREADME
!define MUI_FINISHPAGE_SHOWREADME_TEXT "创建桌面快捷方式"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION CreateOrUpdateDesktopShortcut
{{desktop_shortcut_notchecked}}
!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipIfPassive
!insertmacro MUI_PAGE_FINISH

; Uninstaller Pages
Var DeleteAppDataCheckbox
Var DeleteAppDataCheckboxState
!define /ifndef WS_EX_LAYOUTRTL 0x00400000
!define MUI_PAGE_CUSTOMFUNCTION_SHOW un.ConfirmShow
Function un.ConfirmShow
  FindWindow $1 "#32770" "" $HWNDPARENT
  System::Call "user32::GetDpiForWindow(p r1) i .r2"
  ${If} $(^RTL) = 1
    StrCpy $3 "${__NSD_CheckBox_EXSTYLE} | ${WS_EX_LAYOUTRTL}"
    IntOp $4 50 * $2
  ${Else}
    StrCpy $3 "${__NSD_CheckBox_EXSTYLE}"
    IntOp $4 0 * $2
  ${EndIf}
  IntOp $5 100 * $2
  IntOp $6 400 * $2
  IntOp $7 25 * $2
  IntOp $4 $4 / 96
  IntOp $5 $5 / 96
  IntOp $6 $6 / 96
  IntOp $7 $7 / 96
  System::Call 'user32::CreateWindowEx(i r3, w "${__NSD_CheckBox_CLASS}", w "清除程序数据（${APP_ID}）", i ${__NSD_CheckBox_STYLE}, i r4, i r5, i r6, i r7, p r1, i0, i0, i0) i .s'
  Pop $DeleteAppDataCheckbox
  SendMessage $HWNDPARENT ${WM_GETFONT} 0 0 $1
  SendMessage $DeleteAppDataCheckbox ${WM_SETFONT} $1 1
FunctionEnd
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE un.ConfirmLeave
Function un.ConfirmLeave
  SendMessage $DeleteAppDataCheckbox ${BM_GETCHECK} 0 0 $DeleteAppDataCheckboxState
FunctionEnd
!define MUI_PAGE_CUSTOMFUNCTION_PRE un.SkipIfPassive
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Languages
{{language_macros}}
!insertmacro MUI_RESERVEFILE_LANGDLL

; --- Helper: Read from HKLM or HKCU ---
!macro ReadRegStrSHCTX var subkey name
  ReadRegStr ${var} HKLM "${subkey}" "${name}"
  ${If} ${var} == ""
    ReadRegStr ${var} HKCU "${subkey}" "${name}"
  ${EndIf}
!macroend

!macro WriteRegStrSHCTX subkey name value
  !if "{{install_scope}}" == "perMachine"
    WriteRegStr HKLM "${subkey}" "${name}" "${value}"
  !else
    WriteRegStr HKCU "${subkey}" "${name}" "${value}"
  !endif
!macroend

!macro DeleteRegKeySHCTX subkey
  DeleteRegKey HKLM "${subkey}"
  DeleteRegKey HKCU "${subkey}"
!macroend

; --- SetContext macro ---
!macro SetContext
  !if "{{install_scope}}" == "perMachine"
    SetShellVarContext all
  !else if "{{install_scope}}" == "perUser"
    SetShellVarContext current
  !endif
!macroend

; --- Skip helpers (like Tauri) ---
Function SkipIfPassive
  ${IfThen} $PassiveMode = 1 ${|} Abort ${|}
FunctionEnd

Function un.SkipIfPassive
  ${IfThen} $PassiveMode = 1 ${|} Abort ${|}
FunctionEnd

Function Skip
  Abort
FunctionEnd

; --- Reinstall page functions (strictly following Tauri) ---
Function PageReinstall
  ; Check if there is an existing installation, if not, abort
  StrCpy $R0 ""
  StrCpy $R1 ""
  !insertmacro ReadRegStrSHCTX $R0 "${UNINSTKEY}" ""
  !insertmacro ReadRegStrSHCTX $R1 "${UNINSTKEY}" "UninstallString"
  ${IfThen} "$R0$R1" == "" ${|} Abort ${|}

  ; Compare this installer version with the existing installation
  StrCpy $R0 ""
  !insertmacro ReadRegStrSHCTX $R0 "${UNINSTKEY}" "DisplayVersion"

  nsis_tauri_utils::SemverCompare "${VERSION}" $R0
  Pop $R0

  ; Reinstalling the same version
  ${If} $R0 = 0
    StrCpy $R1 "已安装 ${PRODUCTNAME}。"
    StrCpy $R2 "重新安装"
    StrCpy $R3 "卸载"
    !insertmacro MUI_HEADER_TEXT "已安装" "选择维护选项"
  ; Upgrading
  ${ElseIf} $R0 = 1
    StrCpy $R1 "检测到旧版本 ${PRODUCTNAME}。"
    StrCpy $R2 "卸载后安装"
    StrCpy $R3 "不卸载"
    !insertmacro MUI_HEADER_TEXT "已安装" "选择安装方式"
  ; Downgrading
  ${ElseIf} $R0 = -1
    StrCpy $R1 "检测到新版本 ${PRODUCTNAME}。"
    StrCpy $R2 "卸载后安装"
    StrCpy $R3 "不卸载"
    !insertmacro MUI_HEADER_TEXT "已安装" "选择安装方式"
  ${Else}
    Abort
  ${EndIf}

  ; Skip showing the page if passive
  ${If} $PassiveMode = 1
    Call PageLeaveReinstall
  ${Else}
    nsDialogs::Create 1018
    Pop $R4
    ${IfThen} $(^RTL) = 1 ${|} nsDialogs::SetRTL $(^RTL) ${|}

    ${NSD_CreateLabel} 0 0 100% 24u $R1
    Pop $R1

    ${NSD_CreateRadioButton} 30u 50u -30u 8u $R2
    Pop $R2
    ${NSD_OnClick} $R2 PageReinstallUpdateSelection

    ${NSD_CreateRadioButton} 30u 70u -30u 8u $R3
    Pop $R3
    ${NSD_OnClick} $R3 PageReinstallUpdateSelection

    ; Check the first radio button by default
    ${If} $ReinstallPageCheck <> 2
      SendMessage $R2 ${BM_SETCHECK} ${BST_CHECKED} 0
    ${Else}
      SendMessage $R3 ${BM_SETCHECK} ${BST_CHECKED} 0
    ${EndIf}

    ${NSD_SetFocus} $R2
    nsDialogs::Show
  ${EndIf}
FunctionEnd

Function PageReinstallUpdateSelection
  ${NSD_GetState} $R2 $R1
  ${If} $R1 == ${BST_CHECKED}
    StrCpy $ReinstallPageCheck 1
  ${Else}
    StrCpy $ReinstallPageCheck 2
  ${EndIf}
FunctionEnd

Function PageLeaveReinstall
  ${NSD_GetState} $R2 $R1

  ; In update mode, always proceeds without uninstalling
  ${If} $UpdateMode = 1
    Goto reinst_done
  ${EndIf}

  ; $R0 holds whether same(0)/upgrading(1)/downgrading(-1) version
  ; $ReinstallPageCheck: 1 => first choice, 2 => second choice
  ${If} $R0 = 0 ; Same version
    ${If} $ReinstallPageCheck = 1
      Goto reinst_done
    ${Else}
      Goto reinst_uninstall
    ${EndIf}
  ${ElseIf} $R0 = 1 ; Upgrading
    ${If} $ReinstallPageCheck = 1
      Goto reinst_uninstall
    ${Else}
      Goto reinst_done
    ${EndIf}
  ${ElseIf} $R0 = -1 ; Downgrading
    ${If} $ReinstallPageCheck = 1
      Goto reinst_uninstall
    ${Else}
      Goto reinst_done
    ${EndIf}
  ${EndIf}

  reinst_uninstall:
    HideWindow
    ClearErrors
    !insertmacro ReadRegStrSHCTX $4 "${MANUPRODUCTKEY}" ""
    !insertmacro ReadRegStrSHCTX $R1 "${UNINSTKEY}" "UninstallString"
    ${IfThen} $UpdateMode = 1 ${|} StrCpy $R1 "$R1 /UPDATE" ${|}
    ${IfThen} $PassiveMode = 1 ${|} StrCpy $R1 "$R1 /P" ${|}
    StrCpy $R1 "$R1 _?=$4"
    ExecWait '$R1' $0
    BringToFront

    ${IfThen} ${Errors} ${|} StrCpy $0 2 ${|}

    ; If main binary is gone, uninstall was successful
    ; (exit code 2 is common when uninstaller can't delete itself with _?=)
    ${If} $0 <> 0
      ${If} $0 = 1
        ; User cancelled uninstaller
        Abort
      ${EndIf}
      ${IfNot} ${FileExists} "$INSTDIR\${MAINBINARYNAME}"
        ; Main binary gone = uninstall success, ignore exit code
        Goto reinst_done
      ${EndIf}
      MessageBox MB_ICONEXCLAMATION "无法卸载旧版本（退出码: $0），安装将继续。"
    ${EndIf}
  reinst_done:
FunctionEnd

; --- Finish page functions ---
Function RunMainBinary
  nsis_tauri_utils::RunAsUser "$INSTDIR\${MAINBINARYNAME}" ""
FunctionEnd

Function CreateOrUpdateStartMenuShortcut
  ; Skip creating shortcut if in update mode or no shortcut mode
  ${If} $UpdateMode = 1
  ${OrIf} $NoShortcutMode = 1
    Return
  ${EndIf}
  !insertmacro MUI_STARTMENU_WRITE_BEGIN Application
    CreateShortCut "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}"
  !insertmacro MUI_STARTMENU_WRITE_END
FunctionEnd

Function CreateOrUpdateDesktopShortcut
  ; Skip creating shortcut if in update mode or no shortcut mode
  ${If} $UpdateMode = 1
  ${OrIf} $NoShortcutMode = 1
    Return
  ${EndIf}
  CreateShortCut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}"
FunctionEnd

; --- Restore previous install location ---
Function RestorePreviousInstallLocation
  StrCpy $4 ""
  !insertmacro ReadRegStrSHCTX $4 "${MANUPRODUCTKEY}" ""
  StrCmp $4 "" +2 0
  StrCpy $INSTDIR $4
FunctionEnd

; --- WebView2 detection and install ---
{{webview2_section}}

; --- Sections ---
Section "主程序" SEC_MAIN
  SectionIn RO
  SetOutPath $INSTDIR
  !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}" "${PRODUCTNAME}"
{{file_entries}}
  WriteUninstaller "$INSTDIR\uninstall.exe"
  !insertmacro WriteRegStrSHCTX "${UNINSTKEY}" "DisplayName" "${PRODUCTNAME}"
  !insertmacro WriteRegStrSHCTX "${UNINSTKEY}" "DisplayIcon" "$\"$INSTDIR\${MAINBINARYNAME}$\""
  !insertmacro WriteRegStrSHCTX "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  !insertmacro WriteRegStrSHCTX "${UNINSTKEY}" "Publisher" "${PRODUCTNAME}"
  !insertmacro WriteRegStrSHCTX "${UNINSTKEY}" "InstallLocation" "$\"$INSTDIR$\""
  !insertmacro WriteRegStrSHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\uninstall.exe$\""
  !insertmacro WriteRegStrSHCTX "${MANUPRODUCTKEY}" "" $INSTDIR
  Call CreateOrUpdateStartMenuShortcut
  ; Create desktop shortcut for passive mode
  ${If} $PassiveMode = 1
    Call CreateOrUpdateDesktopShortcut
  ${EndIf}
{{file_associations_install}}
SectionEnd

Section Uninstall
{{file_associations_uninstall}}
  !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}" "${PRODUCTNAME}"
  RMDir /r "$INSTDIR"

  ; Remove shortcuts if not updating
  ${If} $UpdateMode <> 1
    Delete "$DESKTOP\${PRODUCTNAME}.lnk"
    !insertmacro MUI_STARTMENU_GETFOLDER Application $AppStartMenuFolder
    Delete "$SMPROGRAMS\$AppStartMenuFolder\${PRODUCTNAME}.lnk"
    RMDir "$SMPROGRAMS\$AppStartMenuFolder"
  ${EndIf}

  ; Delete app data if the checkbox is selected and not updating
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RmDir /r "$APPDATA\${APP_ID}"
    RmDir /r "$LOCALAPPDATA\${APP_ID}"
    DeleteRegKey SHCTX "${MANUPRODUCTKEY}"
  ${EndIf}

  !insertmacro DeleteRegKeySHCTX "${UNINSTKEY}"

  ; Auto close if passive mode or updating
  ${If} $PassiveMode = 1
  ${OrIf} $UpdateMode = 1
    SetAutoClose true
  ${EndIf}
SectionEnd

Function .onInit
  StrCpy $PassiveMode 0
  StrCpy $UpdateMode 0
  StrCpy $NoShortcutMode 0

  ${GetOptions} $CMDLINE "/P" $PassiveMode
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/NS" $NoShortcutMode
  ${IfNot} ${Errors}
    StrCpy $NoShortcutMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}

  !insertmacro SetContext

  ${If} $INSTDIR == "${PLACEHOLDER_INSTALL_DIR}"
    Call RestorePreviousInstallLocation
  ${EndIf}

  {{oninit_body}}
FunctionEnd

Function un.onInit
  !insertmacro SetContext

  StrCpy $PassiveMode 0
  StrCpy $UpdateMode 0
  StrCpy $DeleteAppDataCheckboxState 0

  ${GetOptions} $CMDLINE "/P" $PassiveMode
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
  ${EndIf}

  ${GetOptions} $CMDLINE "/UPDATE" $UpdateMode
  ${IfNot} ${Errors}
    StrCpy $UpdateMode 1
  ${EndIf}

  ; Skip process check in update mode (installer already checked)
  ${If} $UpdateMode <> 1
    !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}" "${PRODUCTNAME}"
  ${EndIf}

  {{uninit_body}}
FunctionEnd
