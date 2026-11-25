; PaperKit installer hooks (Windows, NSIS). Included by Tauri's installer.
;
; After the files are installed, offer the on-device AI model. The installer
; doesn't download it (a slow or failed 596 MB download mid-install is a bad
; place for it): it leaves a note next to the app, and PaperKit starts the
; verified download with progress the first time it opens.
; Not asked when the model is already there (updates) or in silent installs.

!macro NSIS_HOOK_POSTINSTALL
  IfFileExists "$APPDATA\app.paperkit.desktop\models\gliner_multi-v2.1\onnx\model_fp16.onnx" paperkit_ai_done 0
  IfFileExists "$INSTDIR\resources\models\gliner_multi-v2.1\onnx\model_fp16.onnx" paperkit_ai_done 0
  IfFileExists "$INSTDIR\models\gliner_multi-v2.1\onnx\model_fp16.onnx" paperkit_ai_done 0
  MessageBox MB_YESNO|MB_ICONQUESTION "Also download PaperKit's on-device AI model (596 MB)?$\r$\n$\r$\nIt finds names and addresses when you redact. It runs entirely on this computer: your documents never leave it.$\r$\n$\r$\nThe download starts when PaperKit opens. You can also get it later from the Redact screen." /SD IDNO IDNO paperkit_ai_done
    FileOpen $0 "$INSTDIR\download-ai-model.flag" w
    FileWrite $0 "yes"
    FileClose $0
  paperkit_ai_done:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  Delete "$INSTDIR\download-ai-model.flag"
!macroend
