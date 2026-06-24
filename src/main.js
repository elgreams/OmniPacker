const tabs = document.querySelectorAll(".tab");
const panels = document.querySelectorAll(".tab-content");
const qrLoginToggle = document.getElementById("qr-login-toggle");
const appIdInput = document.getElementById("appid");
const osDropdown = document.getElementById("os-dropdown");
const branchDropdown = document.getElementById("branch-dropdown");
const branchOptionsContainer = document.getElementById("branch-options");
const branchAddInput = document.getElementById("branch-add-input");
const branchAddButton = document.getElementById("branch-add-button");
const steamUsernameInput = document.getElementById("steam-username");
const steamPasswordInput = document.getElementById("steam-password");
const startButton = document.querySelector(".start-button");
const openOutputButton = document.querySelector(".open-output-button");
const addToQueueButton = document.querySelector(".add-to-queue");
const clearQueueButton = document.querySelector(".queue-clear-button");
const queueList = document.getElementById("queue-list");
const consoleOutput = document.querySelector(".console-output");
const qrModalOverlay = document.querySelector(".qr-modal-overlay");
const qrModalOutput = document.querySelector(".qr-modal-output");
const qrCopyButton = document.querySelector(".qr-copy-button");
const qrCloseButton = document.querySelector(".qr-close-button");
const settingsButton = document.querySelector(".settings-button");
const settingsModalOverlay = document.querySelector(".settings-modal-overlay");
const settingsCloseButton = document.querySelector(".settings-close-button");
const skipCompressionToggle = document.getElementById("skip-compression-toggle");
const compressionPasswordToggle = document.getElementById(
  "compression-password-toggle",
);
const compressionPasswordInput = document.getElementById(
  "compression-password-input",
);
const customCompressionArgsInput = document.getElementById("custom-compression-args");
const splitArchiveToggle = document.getElementById("split-archive-toggle");
const splitSizePreset = document.getElementById("split-size-preset");
const splitCustomRow = document.getElementById("split-custom-row");
const splitCustomSize = document.getElementById("split-custom-size");
const splitCustomUnit = document.getElementById("split-custom-unit");
const defaultQrToggle = document.getElementById("default-qr-toggle");
const languageSelect = document.getElementById("language-select");
const saveLoginButton = document.querySelector(".save-login-button");
const deleteLoginButton = document.querySelector(".settings-delete-login-button");
const updateBanner = document.getElementById("update-banner");
const updateBannerText = document.getElementById("update-banner-text");
const updateBannerDownload = document.getElementById("update-banner-download");
const updateBannerSkip = document.getElementById("update-banner-skip");
const updateBannerDismiss = document.getElementById("update-banner-dismiss");
const checkUpdateButton = document.getElementById("settings-check-update");
const updateStatusLabel = document.getElementById("settings-update-status");
const settingsVersionLabel = document.getElementById("settings-version");
const steamGuardModalOverlay = document.querySelector(".steam-guard-modal-overlay");
const steamGuardEmailOverlay = document.querySelector(".steam-guard-email-overlay");
const steamGuardEmailMessage = document.querySelector(".steam-guard-email-message");
const steamGuardEmailInput = document.querySelector(".steam-guard-email-input");
const steamGuardEmailSubmitButton = document.querySelector(
  ".steam-guard-email-submit",
);
const steamGuardEmailCloseButton = document.querySelector(".steam-guard-email-close");
const steamGuardEmailStatus = document.querySelector(".steam-guard-email-status");
const templateEditorButton = document.querySelector(".template-editor-button");
const templateModalOverlay = document.querySelector(".template-modal-overlay");
const templateCloseButton = document.querySelector(".template-close-button");
const templateBlocksContainer = document.querySelector(".template-blocks");
const templateBlockSelect = document.querySelector(".template-block-select");
const templateAddBlockButton = document.querySelector(".template-add-block-button");
const templateLoadButton = document.querySelector(".template-load-button");
const templateSaveButton = document.querySelector(".template-save-button");
const templateOpenFolderButton = document.querySelector(".template-open-folder-button");
const templateUnsavedNotice = document.querySelector(".template-unsaved-notice");
const profileDropdown = document.getElementById("profile-dropdown");
const profileOptionsContainer = document.querySelector(".template-profile-panel");
const uploaderNameInput = document.getElementById("uploader-name-input");
const templateLoadInput = document.querySelector(".template-load-input");
const templateCopyButton = document.querySelector(".template-copy-button");
const templatePreviewOutput = document.querySelector(".template-preview-output");
const templatePreviewMeta = document.querySelector(".template-preview-meta");
const templateStatus = document.querySelector(".template-status");
const templateSaveOverlay = document.querySelector(".template-save-overlay");
const templateSaveNameInput = document.getElementById("template-save-name");
const templateSaveError = document.querySelector(".template-save-error");
const templateSaveConfirmButton = document.querySelector(".template-save-confirm");
const templateSaveCancelButton = document.querySelector(".template-save-cancel");
const templateGenericConfirmOverlay = document.querySelector(".template-generic-confirm-overlay");
const templateGenericConfirmTitle = document.querySelector(".template-generic-confirm-title");
const templateGenericConfirmMessage = document.querySelector(".template-generic-confirm-message");
const templateGenericConfirmYes = document.querySelector(".template-generic-confirm-yes");
const templateGenericConfirmNo = document.querySelector(".template-generic-confirm-no");
const outputConflictOverlay = document.querySelector(".output-conflict-overlay");
const outputConflictMessage = document.querySelector(".output-conflict-message");
const outputConflictPath = document.querySelector(".output-conflict-path");
const outputConflictOverwriteButton = document.querySelector(
  ".output-conflict-overwrite",
);
const outputConflictCopyButton = document.querySelector(".output-conflict-copy");
const outputConflictCancelButton = document.querySelector(
  ".output-conflict-cancel",
);

const jobState = {
  jobs: new Map(),
  runningJobId: null,
  selectedJobId: null,
  order: [],
};

const authState = {
  rememberedUsername: null,
  savedLogin: null,
};

const settingsState = {
  skipCompression: false,
  compressionPasswordEnabled: false,
  compressionPassword: "",
  customCompressionArgs: "",
  splitArchiveEnabled: false,
  // 7-Zip volume size token, e.g. "1g", "4700m", "10g". When the preset is
  // "custom", this is derived from splitCustomSize + splitCustomUnit.
  splitSizePreset: "1g",
  splitCustomSize: "",
  splitCustomUnit: "m",
  defaultQrLogin: false,
  language: "en",
  // Global uploader handle, inserted into the {{username}} token by any profile
  // (built-in crew or custom) that references it.
  uploaderName: "",
  // Names of the template profiles selected for generation. Built-in "standard"
  // and "crew" are always available; custom profiles live on disk. Each selected
  // profile produces its own .txt next to a job's output.
  selectedProfiles: ["standard"],
  // Version the user chose to skip via the update banner ("Skip this version").
  // The banner won't show again until a release newer than this appears.
  skippedUpdateVersion: null,
};

const outputConflictState = {
  jobId: null,
  outputName: "",
  outputPath: "",
  busy: false,
};

const steamGuardEmailState = {
  jobId: null,
  provider: "",
  busy: false,
};

const steamGuardEmailRetryState = {
  jobId: null,
  requested: false,
};

const LOG_LINE_CAP = 10000;
const LOG_TRIM_MARGIN = 1000;
const CONSOLE_FLUSH_INTERVAL_MS = 120;

let activeTab =
  document.querySelector(".tab.active")?.dataset.tab || "queue";

const consoleRenderState = {
  jobId: null,
  renderedLines: 0,
  needsFullRender: true,
  timer: null,
};

const translations = {
  en: {
    "tab.queue": "Queue",
    "tab.console": "Console",
    "queue.label": "Queue:",
    "queue.clear": "Clear Queue",
    "queue.empty": "No downloads queued.",
    "queue.running": "Queue is running",
    "queue.noReorder": "Cannot reorder/remove while running",
    "queue.atTop": "Already at top",
    "queue.atBottom": "Already at bottom",
    "queue.appId": "AppID {{appId}}",
    "queue.meta": "Branch: {{branch}} • OS: {{os}}",
    "queue.status.queued": "queued",
    "queue.status.running": "running",
    "queue.status.compressing": "compressing",
    "queue.status.done": "done",
    "queue.status.failed": "failed",
    "auth.username": "Steam Username:",
    "auth.password": "Steam Password:",
    "auth.qr": "Use QR Login",
    "auth.save": "Save Login Details",
    "auth.saveMissing": "Enter both Steam username and password before saving.",
    "auth.saveFailed": "Failed to save login data: {{error}}",
    "auth.deleteFailed": "Failed to delete saved login data: {{error}}",
    "auth.loadFailed": "Failed to load saved login data: {{error}}",
    "auth.tauriUnavailable": "Tauri invoke API unavailable.",
    "auth.reuseQr": "[system] Reusing QR login for {{username}}.",
    "game.title": "Game Manager",
    "game.appid": "AppID:",
    "game.add": "Add to Queue",
    "game.os": "OS:",
    "branch.title": "Branch Manager",
    "branch.label": "Branch to download:",
    "branch.add": "Add",
    "branch.addPlaceholder": "Type a branch name",
    "branch.passwordPlaceholder": "Password",
    "branch.remove": "Remove branch",
    "dropdown.none": "None selected",
    "output.open": "Open Output Folder",
    "output.conflict.title": "Output already exists",
    "output.conflict.message": "An output named \"{{name}}\" already exists.",
    "output.conflict.path": "Location: {{path}}",
    "output.conflict.overwrite": "Overwrite",
    "output.conflict.copy": "Make Copy",
    "output.conflict.cancel": "Cancel",
    "output.conflict.log":
      "[system] Output already exists: {{path}}. Choose overwrite, copy, or cancel.",
    "output.conflict.choice.overwrite":
      "[system] Overwriting existing output.",
    "output.conflict.choice.copy":
      "[system] Creating a copy output.",
    "output.conflict.choice.cancel":
      "[system] Output conflict cancelled by user.",
    "output.conflict.resolveError":
      "[system] Output conflict response failed: {{error}}",
    "console.noSelection": "No job selected.",
    "console.noQueuedJobs": "No queued jobs.",
    "qr.title": "Steam QR Login",
    "qr.help": "Scan with Steam Mobile App",
    "qr.waiting": "Waiting for QR code output…",
    "settings.button": "Settings",
    "settings.title": "Settings",
    "settings.language": "Language:",
    "settings.skipCompression": "Skip compression after download",
    "settings.compressionPasswordToggle": "Use compression password",
    "settings.compressionPasswordLabel": "Compression password:",
    "settings.compressionPasswordRequired":
      "Compression password cannot be enabled without setting a password.",
    "settings.compressionPasswordHint":
      "Header encryption (-mhe=on) is enabled by default when a password is set.",
    "settings.customCompressionArgsLabel": "Custom 7-Zip arguments:",
    "settings.customCompressionArgsHint":
      "Extra flags passed to 7-Zip. OmniPacker auto-tunes -mmt and -md for your system; overriding them may cause issues. Flags -t, -p, -bsp, and -v are not allowed (use the split setting for -v).",
    "settings.splitArchiveToggle": "Split archive into volumes",
    "settings.splitPresetDvd": "4.7 GB (DVD)",
    "settings.splitPresetCustom": "Custom…",
    "settings.splitArchiveHint":
      "Splits the output into multiple volumes (archive.7z.001, .002, …) of the chosen size. Reassemble by extracting the .001 file.",
    "settings.defaultQrLogin": "Default to QR Login",
    "settings.uploaderName": "Uploader name:",
    "settings.uploaderNameHint": "Used for the {{username}} token in templates.",
    "settings.deleteLogin": "Delete Saved Login Data",
    "update.available": "OmniPacker v{{version}} is available.",
    "update.download": "Download",
    "update.skip": "Skip this version",
    "update.dismiss": "✕",
    "update.checkButton": "Check for Updates",
    "update.checking": "Checking for updates…",
    "update.upToDate": "You're up to date.",
    "update.checkFailed": "Update check failed.",
    "template.button": "Template Editor",
    "template.title": "Template Editor",
    "template.builder": "Block Builder",
    "template.preview": "Live Preview",
    "template.addBlock": "Add Block",
    "template.copy": "Copy BBCode",
    "template.save": "Save Template",
    "template.import": "Import JSON",
    "template.openFolder": "Open Profile Folder",
    "template.unsaved": "Unsaved changes. Save this template to add it to your profiles.",
    "template.profile.label": "Profile:",
    "template.profile.editHint": "Load this profile into the editor",
    "template.profile.generateHint": "Checked profiles are generated for each job",
    "template.profile.deleteHint": "Delete this profile",
    "template.save.title": "Save Template Profile",
    "template.save.nameLabel": "Profile name:",
    "template.save.confirm": "Save",
    "template.save.errorEmpty": "Enter a profile name.",
    "template.save.errorReserved": "That name is reserved. Choose a different name.",
    "template.overwrite.title": "Overwrite Profile",
    "template.overwrite.message": "A profile named \"{{name}}\" already exists. Overwrite it?",
    "template.delete.title": "Delete Profile",
    "template.delete.message": "Delete the profile \"{{name}}\"? This cannot be undone.",
    "template.unsavedConfirm.title": "Unsaved Changes",
    "template.unsavedConfirm.message": "You have unsaved changes to this template. Discard them?",
    "template.block.title": "Title Block",
    "template.block.version": "Version Block",
    "template.block.depot_list": "Depot List Block",
    "template.block.free_text": "Free Text Block",
    "template.block.uploaded_version": "Uploaded Version Block",
    "template.block.advertise_omnipacker": "Advertise OmniPacker",
    "template.field.template": "Template",
    "template.field.text": "Text",
    "template.field.depotTitle": "Spoiler title",
    "template.field.depotLine": "Depot line template",
    "template.field.useCode": "Wrap in [code=text]",
    "template.help.chipHint": "Insert:",
    "template.help.crewOnly": "(set from the Uploader name setting)",
    "template.help.field.game_name": "The game's name (e.g. Balatro).",
    "template.help.field.os": "The operating system you downloaded for (e.g. Windows x64).",
    "template.help.field.branch": "The Steam branch (e.g. public).",
    "template.help.field.build_datetime_utc": "When this build was published, in UTC.",
    "template.help.field.build_id": "Steam's numeric build ID for this version.",
    "template.help.field.app_id": "The Steam App ID number.",
    "template.help.field.game_description": "The game's store description text.",
    "template.help.field.website": "The game's official website URL.",
    "template.help.field.username": "Your uploader name from Settings.",
    "template.help.field.depot_id": "The depot's numeric ID.",
    "template.help.field.depot_name": "The depot's name.",
    "template.help.field.manifest_id": "The depot's manifest ID number.",
    "template.action.up": "Move up",
    "template.action.down": "Move down",
    "template.action.remove": "Remove",
    "template.status.ready": "Template ready.",
    "template.status.loaded": "Template loaded.",
    "template.status.saved": "Profile saved.",
    "template.status.deleted": "Profile deleted.",
    "template.status.imported": "Profile imported.",
    "template.error.noMetadata": "No job metadata available. Run a job to preview.",
    "template.error.invalidField": "Unsupported field(s): {{fields}}",
    "template.error.depotLimit": "Depot count exceeds limit of {{limit}}.",
    "template.error.lengthLimit": "Rendered output exceeds {{limit}} characters.",
    "template.error.noDepots": "No depots available for preview.",
    "template.error.invalidFile": "Invalid template file.",
    "template.error.loadFailed": "Failed to load template: {{error}}",
    "template.error.saveFailed": "Failed to save template: {{error}}",
    "template.error.deleteFailed": "Failed to delete profile: {{error}}",
    "template.error.openFolderFailed": "Failed to open profile folder: {{error}}",
    "template.preview.metaReady": "Using metadata from the last completed job.",
    "template.preview.metaMissing": "Preview requires a completed job to supply metadata.",
    "template.preview.metaDefault": "Using example metadata for preview. Run a job to use real data.",
    "steamGuard.title": "Steam Guard Confirmation",
    "steamGuard.message": "STEAM GUARD! Use the Steam Mobile App to confirm your sign in...",
    "steamGuard.waiting": "Waiting for confirmation...",
    "steamGuard.email.title": "Steam Guard Email Code",
    "steamGuard.email.status": "Enter the code from your email.",
    "steamGuard.email.empty": "Enter the email code before submitting.",
    "steamGuard.email.sent": "[system] Steam Guard email code submitted.",
    "steamGuard.email.incorrect":
      "[system] Steam Guard email code incorrect. Restarting queue to request a new code.",
    "steamGuard.email.failed": "[system] Steam Guard email code failed: {{error}}",
    "action.start": "Start",
    "action.cancel": "Cancel",
    "action.copy": "Copy",
    "action.close": "Close",
    "action.submit": "Submit",
    "action.yes": "Yes",
    "action.no": "No",
    "language.en": "English",
    "language.es": "Spanish",
    "language.fr": "French",
    "language.ru": "Russian",
    "language.de": "German",
    "os.windowsX64": "Windows x64",
    "os.windowsX86": "Windows x86",
    "os.linux": "Linux",
    "os.linuxX64": "Linux x64",
    "os.linuxX86": "Linux x86",
    "os.macos": "macOS",
    "os.macosX64": "macOS x64",
    "alert.clearQueue": "Cannot clear queue while a job is running. Cancel the job first.",
    "dd.noDepots":
      "[system] DepotDownloader reported no depots. Verify the OS dropdown matches the target app (Windows x64 is typical).",
    "job.runningMessage": "A download is already running. Please wait or cancel.",
    "job.tauriUnavailable": "Tauri invoke API unavailable. Unable to start job.",
    "job.noQueued": "No queued jobs.",
    "job.outputDir": "Output directory: {{path}}",
    "job.outputDirError": "Output directory lookup failed: {{error}}",
    "job.selectedOs": "[system] Selected OS: {{os}}",
    "job.starting": "Starting DepotDownloader for AppID {{appId}}...",
    "job.startFailed": "Start failed: {{error}}",
    "job.canceling": "[system] Cancelling job...",
    "job.cancelled": "[system] Job cancelled by user.",
    "job.cancelFailed": "[system] Cancel failed: {{error}}",
    "output.unavailable": "Tauri invoke API unavailable. Unable to open folder.",
    "output.failed": "Failed to open output folder: {{error}}",
    "zip.status": "[system] 7-Zip status: {{status}}",
  },
  es: {
    "tab.queue": "Cola",
    "tab.console": "Consola",
    "queue.label": "Cola:",
    "queue.clear": "Limpiar cola",
    "queue.empty": "No hay descargas en cola.",
    "queue.running": "La cola está en ejecución",
    "queue.noReorder": "No se puede reordenar/eliminar mientras se ejecuta",
    "queue.atTop": "Ya está arriba",
    "queue.atBottom": "Ya está abajo",
    "queue.appId": "AppID {{appId}}",
    "queue.meta": "Rama: {{branch}} • SO: {{os}}",
    "queue.status.queued": "en cola",
    "queue.status.running": "en ejecución",
    "queue.status.compressing": "comprimiendo",
    "queue.status.done": "completado",
    "queue.status.failed": "fallido",
    "auth.username": "Usuario de Steam:",
    "auth.password": "Contraseña de Steam:",
    "auth.qr": "Usar inicio de sesión por QR",
    "auth.save": "Guardar datos de inicio de sesión",
    "auth.saveMissing":
      "Ingrese el usuario y la contraseña de Steam antes de guardar.",
    "auth.saveFailed":
      "No se pudieron guardar los datos de inicio de sesión: {{error}}",
    "auth.deleteFailed":
      "No se pudo eliminar los datos de inicio de sesión guardados: {{error}}",
    "auth.loadFailed":
      "No se pudieron cargar los datos de inicio de sesión guardados: {{error}}",
    "auth.tauriUnavailable": "La API invoke de Tauri no está disponible.",
    "auth.reuseQr": "[system] Reutilizando el inicio de sesión por QR para {{username}}.",
    "game.title": "Gestor de juegos",
    "game.appid": "AppID:",
    "game.add": "Agregar a la cola",
    "game.os": "SO:",
    "branch.title": "Gestor de ramas",
    "branch.label": "Rama para descargar:",
    "branch.add": "Agregar",
    "branch.addPlaceholder": "Escriba un nombre de rama",
    "branch.passwordPlaceholder": "Contraseña",
    "branch.remove": "Eliminar rama",
    "dropdown.none": "Ninguno seleccionado",
    "output.open": "Abrir carpeta de salida",
    "output.conflict.title": "La salida ya existe",
    "output.conflict.message":
      "Ya existe una salida llamada \"{{name}}\".",
    "output.conflict.path": "Ubicación: {{path}}",
    "output.conflict.overwrite": "Sobrescribir",
    "output.conflict.copy": "Crear copia",
    "output.conflict.cancel": "Cancelar",
    "output.conflict.log":
      "[system] La salida ya existe: {{path}}. Elige sobrescribir, copiar o cancelar.",
    "output.conflict.choice.overwrite":
      "[system] Sobrescribiendo la salida existente.",
    "output.conflict.choice.copy":
      "[system] Creando una salida de copia.",
    "output.conflict.choice.cancel":
      "[system] Conflicto de salida cancelado por el usuario.",
    "output.conflict.resolveError":
      "[system] Falló la respuesta del conflicto de salida: {{error}}",
    "console.noSelection": "No hay ninguna tarea seleccionada.",
    "console.noQueuedJobs": "No hay tareas en cola.",
    "qr.title": "Inicio de sesión QR de Steam",
    "qr.help": "Escanee con la app móvil de Steam",
    "qr.waiting": "Esperando la salida del código QR…",
    "settings.button": "Configuración",
    "settings.title": "Configuración",
    "settings.language": "Idioma:",
    "settings.skipCompression": "Omitir compresión después de la descarga",
    "settings.compressionPasswordToggle": "Usar contraseña de compresión",
    "settings.compressionPasswordLabel": "Contraseña de compresión:",
    "settings.compressionPasswordRequired":
      "La contraseña de compresión no puede habilitarse sin establecer una contraseña.",
    "settings.compressionPasswordHint":
      "El cifrado de encabezados (-mhe=on) se activa por defecto con contraseña.",
    "settings.customCompressionArgsLabel": "Argumentos personalizados de 7-Zip:",
    "settings.customCompressionArgsHint":
      "Flags adicionales para 7-Zip. OmniPacker ajusta -mmt y -md automáticamente; modificarlos puede causar problemas. Los flags -t, -p, -bsp y -v no están permitidos (usa la opción de división para -v).",
    "settings.splitArchiveToggle": "Dividir archivo en volúmenes",
    "settings.splitPresetDvd": "4.7 GB (DVD)",
    "settings.splitPresetCustom": "Personalizado…",
    "settings.splitArchiveHint":
      "Divide la salida en varios volúmenes (archive.7z.001, .002, …) del tamaño elegido. Para reensamblar, extrae el archivo .001.",
    "settings.defaultQrLogin": "Usar QR de forma predeterminada",
    "settings.uploaderName": "Nombre del autor:",
    "settings.uploaderNameHint": "Se usa para el token {{username}} en las plantillas.",
    "settings.deleteLogin": "Eliminar datos de inicio de sesión guardados",
    "update.available": "OmniPacker v{{version}} ya está disponible.",
    "update.download": "Descargar",
    "update.skip": "Omitir esta versión",
    "update.dismiss": "✕",
    "update.checkButton": "Buscar actualizaciones",
    "update.checking": "Buscando actualizaciones…",
    "update.upToDate": "Estás actualizado.",
    "update.checkFailed": "Error al buscar actualizaciones.",
    "template.button": "Editor de plantillas",
    "template.title": "Editor de plantillas",
    "template.builder": "Constructor de bloques",
    "template.preview": "Vista previa en vivo",
    "template.addBlock": "Agregar bloque",
    "template.copy": "Copiar BBCode",
    "template.save": "Guardar plantilla",
    "template.import": "Importar JSON",
    "template.openFolder": "Abrir carpeta de perfiles",
    "template.unsaved": "Cambios sin guardar. Guarda esta plantilla para añadirla a tus perfiles.",
    "template.profile.label": "Perfil:",
    "template.profile.editHint": "Cargar este perfil en el editor",
    "template.profile.generateHint": "Los perfiles marcados se generan en cada trabajo",
    "template.profile.deleteHint": "Eliminar este perfil",
    "template.save.title": "Guardar perfil de plantilla",
    "template.save.nameLabel": "Nombre del perfil:",
    "template.save.confirm": "Guardar",
    "template.save.errorEmpty": "Introduce un nombre de perfil.",
    "template.save.errorReserved": "Ese nombre está reservado. Elige otro nombre.",
    "template.overwrite.title": "Sobrescribir perfil",
    "template.overwrite.message": "Ya existe un perfil llamado \"{{name}}\". ¿Sobrescribirlo?",
    "template.delete.title": "Eliminar perfil",
    "template.delete.message": "¿Eliminar el perfil \"{{name}}\"? Esto no se puede deshacer.",
    "template.unsavedConfirm.title": "Cambios sin guardar",
    "template.unsavedConfirm.message": "Tienes cambios sin guardar en esta plantilla. ¿Descartarlos?",
    "template.block.title": "Bloque de título",
    "template.block.version": "Bloque de versión",
    "template.block.depot_list": "Bloque de lista de depósitos",
    "template.block.free_text": "Bloque de texto libre",
    "template.block.uploaded_version": "Bloque de versión subida",
    "template.block.advertise_omnipacker": "Anunciar OmniPacker",
    "template.field.template": "Plantilla",
    "template.field.text": "Texto",
    "template.field.depotTitle": "Título del spoiler",
    "template.field.depotLine": "Plantilla de línea de depósito",
    "template.field.useCode": "Envolver en [code=text]",
    "template.help.chipHint": "Insertar:",
    "template.help.crewOnly": "(se define en el ajuste Nombre del autor)",
    "template.help.field.game_name": "El nombre del juego (p. ej. Balatro).",
    "template.help.field.os": "El sistema operativo que descargaste (p. ej. Windows x64).",
    "template.help.field.branch": "La rama de Steam (p. ej. public).",
    "template.help.field.build_datetime_utc": "Cuándo se publicó esta compilación, en UTC.",
    "template.help.field.build_id": "El ID numérico de compilación de Steam para esta versión.",
    "template.help.field.app_id": "El número de App ID de Steam.",
    "template.help.field.game_description": "El texto de descripción de la tienda del juego.",
    "template.help.field.website": "La URL del sitio web oficial del juego.",
    "template.help.field.username": "Tu nombre de uploader desde Ajustes.",
    "template.help.field.depot_id": "El ID numérico del depósito.",
    "template.help.field.depot_name": "El nombre del depósito.",
    "template.help.field.manifest_id": "El número de ID de manifiesto del depósito.",
    "template.action.up": "Mover arriba",
    "template.action.down": "Mover abajo",
    "template.action.remove": "Eliminar",
    "template.status.ready": "Plantilla lista.",
    "template.status.loaded": "Plantilla cargada.",
    "template.status.saved": "Perfil guardado.",
    "template.status.deleted": "Perfil eliminado.",
    "template.status.imported": "Perfil importado.",
    "template.error.noMetadata": "No hay metadatos del trabajo. Ejecute una tarea para previsualizar.",
    "template.error.invalidField": "Campo(s) no compatible(s): {{fields}}",
    "template.error.depotLimit": "La cantidad de depósitos supera el límite de {{limit}}.",
    "template.error.lengthLimit": "La salida generada supera los {{limit}} caracteres.",
    "template.error.noDepots": "No hay depósitos disponibles para la vista previa.",
    "template.error.invalidFile": "Archivo de plantilla no válido.",
    "template.error.loadFailed": "Error al cargar la plantilla: {{error}}",
    "template.error.saveFailed": "Error al guardar la plantilla: {{error}}",
    "template.error.deleteFailed": "Error al eliminar el perfil: {{error}}",
    "template.error.openFolderFailed": "Error al abrir la carpeta de perfiles: {{error}}",
    "template.preview.metaReady": "Usando metadatos del último trabajo completado.",
    "template.preview.metaMissing": "La vista previa requiere un trabajo completado para obtener metadatos.",
    "template.preview.metaDefault": "Usando metadatos de ejemplo para la vista previa. Ejecute una tarea para usar datos reales.",
    "steamGuard.title": "Confirmación de Steam Guard",
    "steamGuard.message":
      "STEAM GUARD! Use la app móvil de Steam para confirmar su inicio de sesión...",
    "steamGuard.waiting": "Esperando confirmación...",
    "steamGuard.email.title": "Steam Guard Email Code",
    "steamGuard.email.status": "Enter the code from your email.",
    "steamGuard.email.empty": "Enter the email code before submitting.",
    "steamGuard.email.sent": "[system] Steam Guard email code submitted.",
    "steamGuard.email.failed": "[system] Steam Guard email code failed: {{error}}",
    "action.start": "Iniciar",
    "action.cancel": "Cancelar",
    "action.copy": "Copiar",
    "action.close": "Cerrar",
    "action.submit": "Submit",
    "action.yes": "Sí",
    "action.no": "No",
    "language.en": "Inglés",
    "language.es": "Español",
    "language.fr": "Francés",
    "language.ru": "Ruso",
    "language.de": "Alemán",
    "os.windowsX64": "Windows x64",
    "os.windowsX86": "Windows x86",
    "os.linux": "Linux",
    "os.linuxX64": "Linux x64",
    "os.linuxX86": "Linux x86",
    "os.macos": "macOS",
    "os.macosX64": "macOS x64",
    "alert.clearQueue":
      "No se puede limpiar la cola mientras hay una tarea en ejecución. Cancele la tarea primero.",
    "dd.noDepots":
      "[system] DepotDownloader no encontró depósitos. Verifique que el desplegable de SO coincida con la app (Windows x64 suele ser lo habitual).",
    "job.runningMessage": "Ya hay una descarga en ejecución. Espere o cancele.",
    "job.tauriUnavailable":
      "La API invoke de Tauri no está disponible. No se puede iniciar la tarea.",
    "job.noQueued": "No hay tareas en cola.",
    "job.outputDir": "Directorio de salida: {{path}}",
    "job.outputDirError": "No se pudo obtener el directorio de salida: {{error}}",
    "job.selectedOs": "[system] SO seleccionado: {{os}}",
    "job.starting": "Iniciando DepotDownloader para AppID {{appId}}...",
    "job.startFailed": "El inicio falló: {{error}}",
    "job.canceling": "[system] Cancelando tarea...",
    "job.cancelled": "[system] Tarea cancelada por el usuario.",
    "job.cancelFailed": "[system] Cancelación fallida: {{error}}",
    "output.unavailable":
      "La API invoke de Tauri no está disponible. No se puede abrir la carpeta.",
    "output.failed": "No se pudo abrir la carpeta de salida: {{error}}",
    "zip.status": "[system] Estado de 7-Zip: {{status}}",
  },
  fr: {
    "tab.queue": "File",
    "tab.console": "Console",
    "queue.label": "File:",
    "queue.clear": "Vider la file",
    "queue.empty": "Aucun téléchargement en file.",
    "queue.running": "La file est en cours",
    "queue.noReorder": "Impossible de réorganiser/supprimer pendant l'exécution",
    "queue.atTop": "Déjà en haut",
    "queue.atBottom": "Déjà en bas",
    "queue.appId": "AppID {{appId}}",
    "queue.meta": "Branche : {{branch}} • OS : {{os}}",
    "queue.status.queued": "en file",
    "queue.status.running": "en cours",
    "queue.status.compressing": "compression",
    "queue.status.done": "terminé",
    "queue.status.failed": "échoué",
    "auth.username": "Nom d'utilisateur Steam:",
    "auth.password": "Mot de passe Steam:",
    "auth.qr": "Connexion par QR",
    "auth.save": "Enregistrer les identifiants",
    "auth.saveMissing":
      "Saisissez le nom d'utilisateur et le mot de passe Steam avant d'enregistrer.",
    "auth.saveFailed":
      "Impossible d'enregistrer les identifiants: {{error}}",
    "auth.deleteFailed":
      "Impossible de supprimer les identifiants enregistrés: {{error}}",
    "auth.loadFailed":
      "Impossible de charger les identifiants enregistrés: {{error}}",
    "auth.tauriUnavailable": "L'API invoke de Tauri est indisponible.",
    "auth.reuseQr": "[system] Réutilisation de la connexion QR pour {{username}}.",
    "game.title": "Gestionnaire de jeux",
    "game.appid": "AppID:",
    "game.add": "Ajouter à la file",
    "game.os": "OS:",
    "branch.title": "Gestionnaire de branches",
    "branch.label": "Branche à télécharger:",
    "branch.add": "Ajouter",
    "branch.addPlaceholder": "Saisir un nom de branche",
    "branch.passwordPlaceholder": "Mot de passe",
    "branch.remove": "Supprimer la branche",
    "dropdown.none": "Aucune sélection",
    "output.open": "Ouvrir le dossier de sortie",
    "output.conflict.title": "La sortie existe déjà",
    "output.conflict.message":
      "Une sortie nommée \"{{name}}\" existe déjà.",
    "output.conflict.path": "Emplacement : {{path}}",
    "output.conflict.overwrite": "Écraser",
    "output.conflict.copy": "Créer une copie",
    "output.conflict.cancel": "Annuler",
    "output.conflict.log":
      "[system] La sortie existe déjà : {{path}}. Choisissez écraser, copier ou annuler.",
    "output.conflict.choice.overwrite":
      "[system] Écrasement de la sortie existante.",
    "output.conflict.choice.copy":
      "[system] Création d'une copie de la sortie.",
    "output.conflict.choice.cancel":
      "[system] Conflit de sortie annulé par l'utilisateur.",
    "output.conflict.resolveError":
      "[system] Échec de la réponse au conflit de sortie : {{error}}",
    "console.noSelection": "Aucune tâche sélectionnée.",
    "console.noQueuedJobs": "Aucune tâche en file.",
    "qr.title": "Connexion QR Steam",
    "qr.help": "Scannez avec l'application mobile Steam",
    "qr.waiting": "En attente de la sortie du code QR…",
    "settings.button": "Paramètres",
    "settings.title": "Paramètres",
    "settings.language": "Langue:",
    "settings.skipCompression": "Ignorer la compression après le téléchargement",
    "settings.compressionPasswordToggle": "Utiliser un mot de passe de compression",
    "settings.compressionPasswordLabel": "Mot de passe de compression :",
    "settings.compressionPasswordRequired":
      "Le mot de passe de compression ne peut pas être activé sans en définir un.",
    "settings.compressionPasswordHint":
      "Le chiffrement des en-têtes (-mhe=on) est activé par défaut avec un mot de passe.",
    "settings.customCompressionArgsLabel": "Arguments 7-Zip personnalisés :",
    "settings.customCompressionArgsHint":
      "Flags supplémentaires pour 7-Zip. OmniPacker ajuste -mmt et -md automatiquement; les modifier peut causer des problèmes. Les flags -t, -p, -bsp et -v ne sont pas autorisés (utilisez l'option de division pour -v).",
    "settings.splitArchiveToggle": "Diviser l'archive en volumes",
    "settings.splitPresetDvd": "4.7 Go (DVD)",
    "settings.splitPresetCustom": "Personnalisé…",
    "settings.splitArchiveHint":
      "Divise la sortie en plusieurs volumes (archive.7z.001, .002, …) de la taille choisie. Pour réassembler, extrayez le fichier .001.",
    "settings.defaultQrLogin": "Utiliser QR par défaut",
    "settings.uploaderName": "Nom de l'uploader :",
    "settings.uploaderNameHint": "Utilisé pour le jeton {{username}} dans les modèles.",
    "settings.deleteLogin": "Supprimer les identifiants enregistrés",
    "update.available": "OmniPacker v{{version}} est disponible.",
    "update.download": "Télécharger",
    "update.skip": "Ignorer cette version",
    "update.dismiss": "✕",
    "update.checkButton": "Rechercher des mises à jour",
    "update.checking": "Recherche de mises à jour…",
    "update.upToDate": "Vous êtes à jour.",
    "update.checkFailed": "Échec de la vérification des mises à jour.",
    "template.button": "Éditeur de modèles",
    "template.title": "Éditeur de modèles",
    "template.builder": "Constructeur de blocs",
    "template.preview": "Aperçu en direct",
    "template.addBlock": "Ajouter un bloc",
    "template.copy": "Copier le BBCode",
    "template.save": "Enregistrer le modèle",
    "template.import": "Importer le JSON",
    "template.openFolder": "Ouvrir le dossier des profils",
    "template.unsaved": "Modifications non enregistrées. Enregistrez ce modèle pour l'ajouter à vos profils.",
    "template.profile.label": "Profil :",
    "template.profile.editHint": "Charger ce profil dans l'éditeur",
    "template.profile.generateHint": "Les profils cochés sont générés pour chaque tâche",
    "template.profile.deleteHint": "Supprimer ce profil",
    "template.save.title": "Enregistrer le profil de modèle",
    "template.save.nameLabel": "Nom du profil :",
    "template.save.confirm": "Enregistrer",
    "template.save.errorEmpty": "Saisissez un nom de profil.",
    "template.save.errorReserved": "Ce nom est réservé. Choisissez un autre nom.",
    "template.overwrite.title": "Écraser le profil",
    "template.overwrite.message": "Un profil nommé « {{name}} » existe déjà. L'écraser ?",
    "template.delete.title": "Supprimer le profil",
    "template.delete.message": "Supprimer le profil « {{name}} » ? Cette action est irréversible.",
    "template.unsavedConfirm.title": "Modifications non enregistrées",
    "template.unsavedConfirm.message": "Vous avez des modifications non enregistrées sur ce modèle. Les abandonner ?",
    "template.block.title": "Bloc de titre",
    "template.block.version": "Bloc de version",
    "template.block.depot_list": "Bloc de liste des dépôts",
    "template.block.free_text": "Bloc de texte libre",
    "template.block.uploaded_version": "Bloc de version téléversée",
    "template.block.advertise_omnipacker": "Promouvoir OmniPacker",
    "template.field.template": "Modèle",
    "template.field.text": "Texte",
    "template.field.depotTitle": "Titre du spoiler",
    "template.field.depotLine": "Modèle de ligne de dépôt",
    "template.field.useCode": "Encadrer dans [code=text]",
    "template.help.chipHint": "Insérer :",
    "template.help.crewOnly": "(défini via le paramètre Nom de l'uploader)",
    "template.help.field.game_name": "Le nom du jeu (p. ex. Balatro).",
    "template.help.field.os": "Le système d'exploitation téléchargé (p. ex. Windows x64).",
    "template.help.field.branch": "La branche Steam (p. ex. public).",
    "template.help.field.build_datetime_utc": "Date de publication de cette version, en UTC.",
    "template.help.field.build_id": "L'identifiant numérique de build Steam de cette version.",
    "template.help.field.app_id": "Le numéro d'App ID Steam.",
    "template.help.field.game_description": "Le texte de description de la boutique du jeu.",
    "template.help.field.website": "L'URL du site officiel du jeu.",
    "template.help.field.username": "Votre nom d'uploader depuis les Paramètres.",
    "template.help.field.depot_id": "L'identifiant numérique du dépôt.",
    "template.help.field.depot_name": "Le nom du dépôt.",
    "template.help.field.manifest_id": "Le numéro d'ID de manifeste du dépôt.",
    "template.action.up": "Monter",
    "template.action.down": "Descendre",
    "template.action.remove": "Supprimer",
    "template.status.ready": "Modèle prêt.",
    "template.status.loaded": "Modèle chargé.",
    "template.status.saved": "Profil enregistré.",
    "template.status.deleted": "Profil supprimé.",
    "template.status.imported": "Profil importé.",
    "template.error.noMetadata": "Aucune métadonnée de tâche disponible. Exécutez une tâche pour prévisualiser.",
    "template.error.invalidField": "Champ(s) non pris en charge : {{fields}}",
    "template.error.depotLimit": "Le nombre de dépôts dépasse la limite de {{limit}}.",
    "template.error.lengthLimit": "La sortie générée dépasse {{limit}} caractères.",
    "template.error.noDepots": "Aucun dépôt disponible pour l'aperçu.",
    "template.error.invalidFile": "Fichier de modèle invalide.",
    "template.error.loadFailed": "Échec du chargement du modèle : {{error}}",
    "template.error.saveFailed": "Échec de l'enregistrement du modèle : {{error}}",
    "template.error.deleteFailed": "Échec de la suppression du profil : {{error}}",
    "template.error.openFolderFailed": "Échec de l'ouverture du dossier des profils : {{error}}",
    "template.preview.metaReady": "Utilisation des métadonnées de la dernière tâche terminée.",
    "template.preview.metaMissing": "L'aperçu nécessite une tâche terminée pour fournir des métadonnées.",
    "template.preview.metaDefault": "Utilisation de métadonnées d'exemple pour l'aperçu. Exécutez une tâche pour utiliser des données réelles.",
    "steamGuard.title": "Confirmation Steam Guard",
    "steamGuard.message":
      "STEAM GUARD! Utilisez l'application mobile Steam pour confirmer votre connexion...",
    "steamGuard.waiting": "En attente de confirmation...",
    "steamGuard.email.title": "Steam Guard Email Code",
    "steamGuard.email.status": "Enter the code from your email.",
    "steamGuard.email.empty": "Enter the email code before submitting.",
    "steamGuard.email.sent": "[system] Steam Guard email code submitted.",
    "steamGuard.email.failed": "[system] Steam Guard email code failed: {{error}}",
    "action.start": "Démarrer",
    "action.cancel": "Annuler",
    "action.copy": "Copier",
    "action.close": "Fermer",
    "action.submit": "Submit",
    "action.yes": "Oui",
    "action.no": "Non",
    "language.en": "Anglais",
    "language.es": "Espagnol",
    "language.fr": "Français",
    "language.ru": "Russe",
    "language.de": "Allemand",
    "os.windowsX64": "Windows x64",
    "os.windowsX86": "Windows x86",
    "os.linux": "Linux",
    "os.linuxX64": "Linux x64",
    "os.linuxX86": "Linux x86",
    "os.macos": "macOS",
    "os.macosX64": "macOS x64",
    "alert.clearQueue":
      "Impossible de vider la file pendant qu'une tâche est en cours. Annulez d'abord la tâche.",
    "dd.noDepots":
      "[system] DepotDownloader n'a trouvé aucun dépôt. Vérifiez que la liste OS correspond à l'application (Windows x64 est généralement le bon choix).",
    "job.runningMessage":
      "Un téléchargement est déjà en cours. Veuillez patienter ou annuler.",
    "job.tauriUnavailable":
      "L'API invoke de Tauri est indisponible. Impossible de démarrer la tâche.",
    "job.noQueued": "Aucune tâche en file.",
    "job.outputDir": "Dossier de sortie: {{path}}",
    "job.outputDirError": "Impossible d'obtenir le dossier de sortie: {{error}}",
    "job.selectedOs": "[system] OS sélectionné: {{os}}",
    "job.starting": "Démarrage de DepotDownloader pour l'AppID {{appId}}...",
    "job.startFailed": "Échec du démarrage: {{error}}",
    "job.canceling": "[system] Annulation de la tâche...",
    "job.cancelled": "[system] Tâche annulée par l'utilisateur.",
    "job.cancelFailed": "[system] Échec de l'annulation: {{error}}",
    "output.unavailable":
      "L'API invoke de Tauri est indisponible. Impossible d'ouvrir le dossier.",
    "output.failed": "Impossible d'ouvrir le dossier de sortie: {{error}}",
    "zip.status": "[system] Statut 7-Zip: {{status}}",
  },
  de: {
    "tab.queue": "Warteschlange",
    "tab.console": "Konsole",
    "queue.label": "Warteschlange:",
    "queue.clear": "Warteschlange leeren",
    "queue.empty": "Keine Downloads in der Warteschlange.",
    "queue.running": "Warteschlange läuft",
    "queue.noReorder": "Reihenfolge/Entfernen während der Ausführung nicht möglich",
    "queue.atTop": "Bereits ganz oben",
    "queue.atBottom": "Bereits ganz unten",
    "queue.appId": "AppID {{appId}}",
    "queue.meta": "Branch: {{branch}} • OS: {{os}}",
    "queue.status.queued": "in Warteschlange",
    "queue.status.running": "läuft",
    "queue.status.compressing": "komprimieren",
    "queue.status.done": "fertig",
    "queue.status.failed": "fehlgeschlagen",
    "auth.username": "Steam-Benutzername:",
    "auth.password": "Steam-Passwort:",
    "auth.qr": "QR-Login verwenden",
    "auth.save": "Login-Daten speichern",
    "auth.saveMissing":
      "Bitte Steam-Benutzername und Passwort eingeben, bevor gespeichert wird.",
    "auth.saveFailed": "Login-Daten konnten nicht gespeichert werden: {{error}}",
    "auth.deleteFailed":
      "Gespeicherte Login-Daten konnten nicht gelöscht werden: {{error}}",
    "auth.loadFailed":
      "Gespeicherte Login-Daten konnten nicht geladen werden: {{error}}",
    "auth.tauriUnavailable": "Tauri invoke API nicht verfügbar.",
    "auth.reuseQr": "[system] QR-Login für {{username}} wird wiederverwendet.",
    "game.title": "Spiel-Manager",
    "game.appid": "AppID:",
    "game.add": "Einreihen",
    "game.os": "OS:",
    "branch.title": "Branch-Manager",
    "branch.label": "Branch zum Download:",
    "branch.add": "Hinzufügen",
    "branch.addPlaceholder": "Branch-Namen eingeben",
    "branch.passwordPlaceholder": "Passwort",
    "branch.remove": "Branch entfernen",
    "dropdown.none": "Nichts ausgewählt",
    "output.open": "Ausgabeordner öffnen",
    "output.conflict.title": "Ausgabe bereits vorhanden",
    "output.conflict.message":
      "Eine Ausgabe namens \"{{name}}\" ist bereits vorhanden.",
    "output.conflict.path": "Speicherort: {{path}}",
    "output.conflict.overwrite": "Überschreiben",
    "output.conflict.copy": "Kopie erstellen",
    "output.conflict.cancel": "Abbrechen",
    "output.conflict.log":
      "[system] Ausgabe bereits vorhanden: {{path}}. Überschreiben, kopieren oder abbrechen.",
    "output.conflict.choice.overwrite":
      "[system] Vorhandene Ausgabe wird überschrieben.",
    "output.conflict.choice.copy":
      "[system] Kopie der Ausgabe wird erstellt.",
    "output.conflict.choice.cancel":
      "[system] Ausgabekonflikt vom Benutzer abgebrochen.",
    "output.conflict.resolveError":
      "[system] Antwort auf Ausgabekonflikt fehlgeschlagen: {{error}}",
    "console.noSelection": "Kein Job ausgewählt.",
    "console.noQueuedJobs": "Keine Jobs in der Warteschlange.",
    "qr.title": "Steam-QR-Login",
    "qr.help": "Mit der Steam-Mobile-App scannen",
    "qr.waiting": "Warte auf QR-Code-Ausgabe…",
    "settings.button": "Einstellungen",
    "settings.title": "Einstellungen",
    "settings.language": "Sprache:",
    "settings.skipCompression": "Komprimierung nach dem Download überspringen",
    "settings.compressionPasswordToggle": "Kompressionspasswort verwenden",
    "settings.compressionPasswordLabel": "Kompressionspasswort:",
    "settings.compressionPasswordRequired":
      "Kompressionspasswort kann nicht aktiviert werden, ohne ein Passwort festzulegen.",
    "settings.compressionPasswordHint":
      "Header-Verschlüsselung (-mhe=on) ist standardmäßig bei Passwort aktiviert.",
    "settings.customCompressionArgsLabel": "Benutzerdefinierte 7-Zip-Argumente:",
    "settings.customCompressionArgsHint":
      "Zusätzliche Flags für 7-Zip. OmniPacker passt -mmt und -md automatisch an; Änderungen können Probleme verursachen. Die Flags -t, -p, -bsp und -v sind nicht erlaubt (für -v die Aufteilen-Option verwenden).",
    "settings.splitArchiveToggle": "Archiv in Teile aufteilen",
    "settings.splitPresetDvd": "4.7 GB (DVD)",
    "settings.splitPresetCustom": "Benutzerdefiniert…",
    "settings.splitArchiveHint":
      "Teilt die Ausgabe in mehrere Teile (archive.7z.001, .002, …) der gewählten Größe auf. Zum Zusammenfügen die .001-Datei entpacken.",
    "settings.defaultQrLogin": "Standardmäßig QR-Login verwenden",
    "settings.uploaderName": "Uploader-Name:",
    "settings.uploaderNameHint": "Wird für den {{username}}-Token in Vorlagen verwendet.",
    "settings.deleteLogin": "Gespeicherte Login-Daten löschen",
    "update.available": "OmniPacker v{{version}} ist verfügbar.",
    "update.download": "Herunterladen",
    "update.skip": "Diese Version überspringen",
    "update.dismiss": "✕",
    "update.checkButton": "Nach Updates suchen",
    "update.checking": "Suche nach Updates…",
    "update.upToDate": "Sie sind auf dem neuesten Stand.",
    "update.checkFailed": "Update-Prüfung fehlgeschlagen.",
    "template.button": "Vorlageneditor",
    "template.title": "Vorlageneditor",
    "template.builder": "Block-Editor",
    "template.preview": "Live-Vorschau",
    "template.addBlock": "Block hinzufügen",
    "template.copy": "BBCode kopieren",
    "template.save": "Vorlage speichern",
    "template.import": "JSON importieren",
    "template.openFolder": "Profilordner öffnen",
    "template.unsaved": "Nicht gespeicherte Änderungen. Speichere diese Vorlage, um sie zu deinen Profilen hinzuzufügen.",
    "template.profile.label": "Profil:",
    "template.profile.editHint": "Dieses Profil in den Editor laden",
    "template.profile.generateHint": "Angehakte Profile werden für jeden Job erzeugt",
    "template.profile.deleteHint": "Dieses Profil löschen",
    "template.save.title": "Vorlagenprofil speichern",
    "template.save.nameLabel": "Profilname:",
    "template.save.confirm": "Speichern",
    "template.save.errorEmpty": "Gib einen Profilnamen ein.",
    "template.save.errorReserved": "Dieser Name ist reserviert. Wähle einen anderen Namen.",
    "template.overwrite.title": "Profil überschreiben",
    "template.overwrite.message": "Ein Profil namens \"{{name}}\" existiert bereits. Überschreiben?",
    "template.delete.title": "Profil löschen",
    "template.delete.message": "Das Profil \"{{name}}\" löschen? Dies kann nicht rückgängig gemacht werden.",
    "template.unsavedConfirm.title": "Nicht gespeicherte Änderungen",
    "template.unsavedConfirm.message": "Du hast nicht gespeicherte Änderungen an dieser Vorlage. Verwerfen?",
    "template.block.title": "Titelblock",
    "template.block.version": "Versionsblock",
    "template.block.depot_list": "Depotlistenblock",
    "template.block.free_text": "Freitextblock",
    "template.block.uploaded_version": "Block für hochgeladene Version",
    "template.block.advertise_omnipacker": "OmniPacker bewerben",
    "template.field.template": "Vorlage",
    "template.field.text": "Text",
    "template.field.depotTitle": "Spoiler-Titel",
    "template.field.depotLine": "Depotzeilen-Vorlage",
    "template.field.useCode": "In [code=text] einschließen",
    "template.help.chipHint": "Einfügen:",
    "template.help.crewOnly": "(über die Einstellung Uploader-Name gesetzt)",
    "template.help.field.game_name": "Der Name des Spiels (z. B. Balatro).",
    "template.help.field.os": "Das heruntergeladene Betriebssystem (z. B. Windows x64).",
    "template.help.field.branch": "Der Steam-Branch (z. B. public).",
    "template.help.field.build_datetime_utc": "Wann dieser Build veröffentlicht wurde, in UTC.",
    "template.help.field.build_id": "Steams numerische Build-ID für diese Version.",
    "template.help.field.app_id": "Die Steam-App-ID-Nummer.",
    "template.help.field.game_description": "Der Shop-Beschreibungstext des Spiels.",
    "template.help.field.website": "Die offizielle Website-URL des Spiels.",
    "template.help.field.username": "Dein Uploader-Name aus den Einstellungen.",
    "template.help.field.depot_id": "Die numerische Depot-ID.",
    "template.help.field.depot_name": "Der Name des Depots.",
    "template.help.field.manifest_id": "Die Manifest-ID-Nummer des Depots.",
    "template.action.up": "Nach oben",
    "template.action.down": "Nach unten",
    "template.action.remove": "Entfernen",
    "template.status.ready": "Vorlage bereit.",
    "template.status.loaded": "Vorlage geladen.",
    "template.status.saved": "Profil gespeichert.",
    "template.status.deleted": "Profil gelöscht.",
    "template.status.imported": "Profil importiert.",
    "template.error.noMetadata": "Keine Job-Metadaten verfügbar. Starte einen Job für die Vorschau.",
    "template.error.invalidField": "Nicht unterstützte Felder: {{fields}}",
    "template.error.depotLimit": "Anzahl der Depots überschreitet das Limit von {{limit}}.",
    "template.error.lengthLimit": "Die Ausgabe überschreitet {{limit}} Zeichen.",
    "template.error.noDepots": "Keine Depots für die Vorschau verfügbar.",
    "template.error.invalidFile": "Ungültige Vorlagendatei.",
    "template.error.loadFailed": "Vorlage konnte nicht geladen werden: {{error}}",
    "template.error.saveFailed": "Vorlage konnte nicht gespeichert werden: {{error}}",
    "template.error.deleteFailed": "Profil konnte nicht gelöscht werden: {{error}}",
    "template.error.openFolderFailed": "Profilordner konnte nicht geöffnet werden: {{error}}",
    "template.preview.metaReady": "Verwendet Metadaten des zuletzt abgeschlossenen Jobs.",
    "template.preview.metaMissing": "Für die Vorschau wird ein abgeschlossener Job benötigt.",
    "template.preview.metaDefault": "Verwendet Beispielmetadaten für die Vorschau. Starte einen Job für echte Daten.",
    "steamGuard.title": "Steam-Guard-Bestätigung",
    "steamGuard.message":
      "STEAM GUARD! Verwenden Sie die Steam-Mobile-App, um Ihren Login zu bestätigen...",
    "steamGuard.waiting": "Warten auf Bestätigung...",
    "steamGuard.email.title": "Steam Guard Email Code",
    "steamGuard.email.status": "Enter the code from your email.",
    "steamGuard.email.empty": "Enter the email code before submitting.",
    "steamGuard.email.sent": "[system] Steam Guard email code submitted.",
    "steamGuard.email.failed": "[system] Steam Guard email code failed: {{error}}",
    "action.start": "Start",
    "action.cancel": "Abbrechen",
    "action.copy": "Kopieren",
    "action.close": "Schließen",
    "action.submit": "Submit",
    "action.yes": "Ja",
    "action.no": "Nein",
    "language.en": "Englisch",
    "language.es": "Spanisch",
    "language.fr": "Französisch",
    "language.ru": "Russisch",
    "language.de": "Deutsch",
    "os.windowsX64": "Windows x64",
    "os.windowsX86": "Windows x86",
    "os.linux": "Linux",
    "os.linuxX64": "Linux x64",
    "os.linuxX86": "Linux x86",
    "os.macos": "macOS",
    "os.macosX64": "macOS x64",
    "alert.clearQueue":
      "Warteschlange kann nicht geleert werden, während ein Job läuft. Brechen Sie den Job zuerst ab.",
    "dd.noDepots":
      "[system] DepotDownloader hat keine Depots gefunden. Stellen Sie sicher, dass das OS-Dropdown zur App passt (Windows x64 ist üblich).",
    "job.runningMessage": "Ein Download läuft bereits. Bitte warten oder abbrechen.",
    "job.tauriUnavailable":
      "Tauri invoke API nicht verfügbar. Job kann nicht gestartet werden.",
    "job.noQueued": "Keine Jobs in der Warteschlange.",
    "job.outputDir": "Ausgabeverzeichnis: {{path}}",
    "job.outputDirError":
      "Ausgabeverzeichnis konnte nicht ermittelt werden: {{error}}",
    "job.selectedOs": "[system] Ausgewähltes OS: {{os}}",
    "job.starting": "DepotDownloader wird für AppID {{appId}} gestartet...",
    "job.startFailed": "Start fehlgeschlagen: {{error}}",
    "job.canceling": "[system] Job wird abgebrochen...",
    "job.cancelled": "[system] Job vom Benutzer abgebrochen.",
    "job.cancelFailed": "[system] Abbruch fehlgeschlagen: {{error}}",
    "output.unavailable":
      "Tauri invoke API nicht verfügbar. Ordner kann nicht geöffnet werden.",
    "output.failed": "Ausgabeordner konnte nicht geöffnet werden: {{error}}",
    "zip.status": "[system] 7-Zip-Status: {{status}}",
  },
  ru: {
    "tab.queue": "Очередь",
    "tab.console": "Консоль",
    "queue.label": "Очередь:",
    "queue.clear": "Очистить очередь",
    "queue.empty": "Нет загрузок в очереди.",
    "queue.running": "Очередь выполняется",
    "queue.noReorder": "Нельзя менять порядок/удалять во время выполнения",
    "queue.atTop": "Уже вверху",
    "queue.atBottom": "Уже внизу",
    "queue.appId": "AppID {{appId}}",
    "queue.meta": "Ветка: {{branch}} • ОС: {{os}}",
    "queue.status.queued": "в очереди",
    "queue.status.running": "выполняется",
    "queue.status.compressing": "сжатие",
    "queue.status.done": "готово",
    "queue.status.failed": "ошибка",
    "auth.username": "Имя пользователя Steam:",
    "auth.password": "Пароль Steam:",
    "auth.qr": "Вход по QR",
    "auth.save": "Сохранить данные входа",
    "auth.saveMissing":
      "Введите имя пользователя и пароль Steam перед сохранением.",
    "auth.saveFailed": "Не удалось сохранить данные входа: {{error}}",
    "auth.deleteFailed": "Не удалось удалить сохраненные данные: {{error}}",
    "auth.loadFailed": "Не удалось загрузить сохраненные данные: {{error}}",
    "auth.tauriUnavailable": "Tauri invoke API недоступен.",
    "auth.reuseQr": "[system] Повторный вход по QR для {{username}}.",
    "game.title": "Менеджер игр",
    "game.appid": "AppID:",
    "game.add": "Добавить в очередь",
    "game.os": "ОС:",
    "branch.title": "Менеджер веток",
    "branch.label": "Ветка для загрузки:",
    "branch.add": "Добавить",
    "branch.addPlaceholder": "Введите имя ветки",
    "branch.passwordPlaceholder": "Пароль",
    "branch.remove": "Удалить ветку",
    "dropdown.none": "Ничего не выбрано",
    "output.open": "Открыть папку вывода",
    "output.conflict.title": "Вывод уже существует",
    "output.conflict.message":
      "Вывод с именем \"{{name}}\" уже существует.",
    "output.conflict.path": "Расположение: {{path}}",
    "output.conflict.overwrite": "Перезаписать",
    "output.conflict.copy": "Создать копию",
    "output.conflict.cancel": "Отмена",
    "output.conflict.log":
      "[system] Вывод уже существует: {{path}}. Выберите перезаписать, копировать или отменить.",
    "output.conflict.choice.overwrite":
      "[system] Перезапись существующего вывода.",
    "output.conflict.choice.copy":
      "[system] Создание копии вывода.",
    "output.conflict.choice.cancel":
      "[system] Конфликт вывода отменен пользователем.",
    "output.conflict.resolveError":
      "[system] Не удалось отправить ответ по конфликту вывода: {{error}}",
    "console.noSelection": "Задание не выбрано.",
    "console.noQueuedJobs": "В очереди нет заданий.",
    "qr.title": "Вход по QR Steam",
    "qr.help": "Сканируйте в мобильном приложении Steam",
    "qr.waiting": "Ожидание вывода QR-кода…",
    "settings.button": "Настройки",
    "settings.title": "Настройки",
    "settings.language": "Язык:",
    "settings.skipCompression": "Пропустить сжатие после загрузки",
    "settings.compressionPasswordToggle": "Использовать пароль для сжатия",
    "settings.compressionPasswordLabel": "Пароль для сжатия:",
    "settings.compressionPasswordRequired":
      "Нельзя включить пароль сжатия без заданного пароля.",
    "settings.compressionPasswordHint":
      "Шифрование заголовков (-mhe=on) включено по умолчанию при наличии пароля.",
    "settings.customCompressionArgsLabel": "Пользовательские аргументы 7-Zip:",
    "settings.customCompressionArgsHint":
      "Дополнительные флаги для 7-Zip. OmniPacker автоматически настраивает -mmt и -md; их изменение может вызвать проблемы. Флаги -t, -p, -bsp и -v запрещены (для -v используйте параметр разделения).",
    "settings.splitArchiveToggle": "Разделить архив на тома",
    "settings.splitPresetDvd": "4.7 ГБ (DVD)",
    "settings.splitPresetCustom": "Другой…",
    "settings.splitArchiveHint":
      "Разделяет вывод на несколько томов (archive.7z.001, .002, …) выбранного размера. Для сборки распакуйте файл .001.",
    "settings.defaultQrLogin": "QR-вход по умолчанию",
    "settings.uploaderName": "Имя загрузившего:",
    "settings.uploaderNameHint": "Используется для токена {{username}} в шаблонах.",
    "settings.deleteLogin": "Удалить сохраненные данные входа",
    "update.available": "Доступна версия OmniPacker v{{version}}.",
    "update.download": "Скачать",
    "update.skip": "Пропустить эту версию",
    "update.dismiss": "✕",
    "update.checkButton": "Проверить обновления",
    "update.checking": "Проверка обновлений…",
    "update.upToDate": "У вас последняя версия.",
    "update.checkFailed": "Не удалось проверить обновления.",
    "template.button": "Редактор шаблонов",
    "template.title": "Редактор шаблонов",
    "template.builder": "Конструктор блоков",
    "template.preview": "Предпросмотр в реальном времени",
    "template.addBlock": "Добавить блок",
    "template.copy": "Копировать BBCode",
    "template.save": "Сохранить шаблон",
    "template.import": "Импортировать JSON",
    "template.openFolder": "Открыть папку профилей",
    "template.unsaved": "Несохранённые изменения. Сохраните этот шаблон, чтобы добавить его в профили.",
    "template.profile.label": "Профиль:",
    "template.profile.editHint": "Загрузить этот профиль в редактор",
    "template.profile.generateHint": "Отмеченные профили генерируются для каждого задания",
    "template.profile.deleteHint": "Удалить этот профиль",
    "template.save.title": "Сохранить профиль шаблона",
    "template.save.nameLabel": "Имя профиля:",
    "template.save.confirm": "Сохранить",
    "template.save.errorEmpty": "Введите имя профиля.",
    "template.save.errorReserved": "Это имя зарезервировано. Выберите другое имя.",
    "template.overwrite.title": "Перезаписать профиль",
    "template.overwrite.message": "Профиль с именем \"{{name}}\" уже существует. Перезаписать?",
    "template.delete.title": "Удалить профиль",
    "template.delete.message": "Удалить профиль \"{{name}}\"? Это действие нельзя отменить.",
    "template.unsavedConfirm.title": "Несохранённые изменения",
    "template.unsavedConfirm.message": "В этом шаблоне есть несохранённые изменения. Отменить их?",
    "template.block.title": "Блок заголовка",
    "template.block.version": "Блок версии",
    "template.block.depot_list": "Блок списка депо",
    "template.block.free_text": "Блок свободного текста",
    "template.block.uploaded_version": "Блок загруженной версии",
    "template.block.advertise_omnipacker": "Реклама OmniPacker",
    "template.field.template": "Шаблон",
    "template.field.text": "Текст",
    "template.field.depotTitle": "Заголовок спойлера",
    "template.field.depotLine": "Шаблон строки депо",
    "template.field.useCode": "Обернуть в [code=text]",
    "template.help.chipHint": "Вставить:",
    "template.help.crewOnly": "(задаётся в настройке «Имя загрузившего»)",
    "template.help.field.game_name": "Название игры (например, Balatro).",
    "template.help.field.os": "Операционная система, для которой вы скачали (например, Windows x64).",
    "template.help.field.branch": "Ветка Steam (например, public).",
    "template.help.field.build_datetime_utc": "Когда была опубликована эта сборка, в UTC.",
    "template.help.field.build_id": "Числовой ID сборки Steam для этой версии.",
    "template.help.field.app_id": "Числовой App ID в Steam.",
    "template.help.field.game_description": "Текст описания игры из магазина.",
    "template.help.field.website": "URL официального сайта игры.",
    "template.help.field.username": "Ваше имя загрузившего из настроек.",
    "template.help.field.depot_id": "Числовой ID депо.",
    "template.help.field.depot_name": "Название депо.",
    "template.help.field.manifest_id": "Числовой ID манифеста депо.",
    "template.action.up": "Вверх",
    "template.action.down": "Вниз",
    "template.action.remove": "Удалить",
    "template.status.ready": "Шаблон готов.",
    "template.status.loaded": "Шаблон загружен.",
    "template.status.saved": "Профиль сохранён.",
    "template.status.deleted": "Профиль удалён.",
    "template.status.imported": "Профиль импортирован.",
    "template.error.noMetadata": "Нет метаданных задания. Запустите задание для предпросмотра.",
    "template.error.invalidField": "Неподдерживаемые поля: {{fields}}",
    "template.error.depotLimit": "Количество депо превышает лимит {{limit}}.",
    "template.error.lengthLimit": "Сгенерированный вывод превышает {{limit}} символов.",
    "template.error.noDepots": "Нет доступных депо для предпросмотра.",
    "template.error.invalidFile": "Некорректный файл шаблона.",
    "template.error.loadFailed": "Не удалось загрузить шаблон: {{error}}",
    "template.error.saveFailed": "Не удалось сохранить шаблон: {{error}}",
    "template.error.deleteFailed": "Не удалось удалить профиль: {{error}}",
    "template.error.openFolderFailed": "Не удалось открыть папку профилей: {{error}}",
    "template.preview.metaReady": "Используются метаданные последнего завершенного задания.",
    "template.preview.metaMissing": "Для предпросмотра требуется завершенное задание.",
    "template.preview.metaDefault": "Используются примерные метаданные для предпросмотра. Запустите задание, чтобы использовать реальные данные.",
    "steamGuard.title": "Подтверждение Steam Guard",
    "steamGuard.message":
      "STEAM GUARD! Используйте мобильное приложение Steam, чтобы подтвердить вход...",
    "steamGuard.waiting": "Ожидание подтверждения...",
    "steamGuard.email.title": "Steam Guard Email Code",
    "steamGuard.email.status": "Enter the code from your email.",
    "steamGuard.email.empty": "Enter the email code before submitting.",
    "steamGuard.email.sent": "[system] Steam Guard email code submitted.",
    "steamGuard.email.failed": "[system] Steam Guard email code failed: {{error}}",
    "action.start": "Старт",
    "action.cancel": "Отмена",
    "action.copy": "Копировать",
    "action.close": "Закрыть",
    "action.submit": "Submit",
    "action.yes": "Да",
    "action.no": "Нет",
    "language.en": "Английский",
    "language.es": "Испанский",
    "language.fr": "Французский",
    "language.ru": "Русский",
    "language.de": "Немецкий",
    "os.windowsX64": "Windows x64",
    "os.windowsX86": "Windows x86",
    "os.linux": "Linux",
    "os.linuxX64": "Linux x64",
    "os.linuxX86": "Linux x86",
    "os.macos": "macOS",
    "os.macosX64": "macOS x64",
    "alert.clearQueue":
      "Нельзя очистить очередь во время выполнения. Сначала отмените задание.",
    "dd.noDepots":
      "[system] DepotDownloader не нашел депоты. Проверьте, что выбранная ОС соответствует приложению (обычно Windows x64).",
    "job.runningMessage": "Загрузка уже выполняется. Подождите или отмените.",
    "job.tauriUnavailable": "Tauri invoke API недоступен. Невозможно запустить задачу.",
    "job.noQueued": "В очереди нет заданий.",
    "job.outputDir": "Каталог вывода: {{path}}",
    "job.outputDirError": "Не удалось определить каталог вывода: {{error}}",
    "job.selectedOs": "[system] Выбранная ОС: {{os}}",
    "job.starting": "Запуск DepotDownloader для AppID {{appId}}...",
    "job.startFailed": "Запуск не удался: {{error}}",
    "job.canceling": "[system] Отмена задания...",
    "job.cancelled": "[system] Задание отменено пользователем.",
    "job.cancelFailed": "[system] Не удалось отменить: {{error}}",
    "output.unavailable": "Tauri invoke API недоступен. Невозможно открыть папку.",
    "output.failed": "Не удалось открыть папку вывода: {{error}}",
    "zip.status": "[system] Статус 7-Zip: {{status}}",
  },
};

const osLabelKeys = {
  "Windows x64": "os.windowsX64",
  "Windows x86": "os.windowsX86",
  "Linux x64": "os.linuxX64",
  "Linux x86": "os.linuxX86",
  Linux: "os.linux",
  macOS: "os.macos",
  "macOS x64": "os.macosX64",
};

const qrWaitingTextValues = Object.values(translations)
  .map((entry) => entry["qr.waiting"])
  .filter(Boolean);

const t = (key, vars = {}) => {
  const table = translations[settingsState.language] ?? translations.en;
  const template = table[key] ?? translations.en[key];
  if (!template) {
    return key;
  }
  return template.replace(/\{\{(\w+)\}\}/g, (_, name) => {
    if (Object.prototype.hasOwnProperty.call(vars, name)) {
      return String(vars[name]);
    }
    return "";
  });
};

const formatStatus = (status) => {
  const translated = t(`queue.status.${status}`);
  return translated || status;
};

const formatOsLabel = (osValue) => {
  const key = osLabelKeys[osValue];
  return key ? t(key) : osValue;
};

const TEMPLATE_MAX_DEPOTS = 100;
const TEMPLATE_MAX_LENGTH = 200000;
const TEMPLATE_SINGLE_FIELDS = [
  "game_name",
  "os",
  "branch",
  "build_datetime_utc",
  "build_id",
  // app_id/game_description/website come from job metadata; username comes from
  // the global "Uploader name" setting.
  "app_id",
  "game_description",
  "website",
  "username",
];
const TEMPLATE_DEPOT_FIELDS = ["depot_id", "depot_name", "manifest_id"];

// Fields sourced from settings rather than job metadata. `username` renders
// blank when the "Uploader name" setting is empty; the chip tooltip notes this.
const TEMPLATE_CREW_ONLY_FIELDS = new Set(["username"]);

// i18n description key per token, shown as a chip tooltip so users know what a
// token does without leaving the editor. Keyed by field name so the chip rows
// (built by iterating the field arrays above) can never drift from the engine.
const templateFieldDescKey = (field) => `template.help.field.${field}`;

// Canonical OmniPacker credit line. Single source of truth on the JS side;
// the backend renderer (template_renderer.rs) has a matching constant.
const OMNIPACKER_CREDIT =
  "Made using [url=https://github.com/elgreams/OmniPacker]OmniPacker[/url]";

// Default test metadata for template preview when no job has been run
const TEMPLATE_DEFAULT_METADATA = {
  game_name: "Balatro",
  os: "Win64",
  branch: "Public",
  build_datetime_utc: "February 24, 2025 - 22:02:36 UTC",
  build_id: "4851806656204679952",
  app_id: "2379780",
  game_description:
    "Balatro is a hypnotically satisfying deckbuilder where you play illegal poker hands.",
  website: "https://www.playbalatro.com",
  depots: [
    {
      depot_id: "228989",
      depot_name: "Steamworks Shared",
      manifest_id: "7206221393165260579",
    },
    {
      depot_id: "2379781",
      depot_name: "Balatro",
      manifest_id: "4851806656204679952",
    },
  ],
};

const TEMPLATE_BLOCK_TYPES = [
  { type: "title", labelKey: "template.block.title" },
  { type: "version", labelKey: "template.block.version" },
  { type: "depot_list", labelKey: "template.block.depot_list" },
  { type: "free_text", labelKey: "template.block.free_text" },
  { type: "uploaded_version", labelKey: "template.block.uploaded_version" },
  { type: "advertise_omnipacker", labelKey: "template.block.advertise_omnipacker" },
];

const TEMPLATE_DEFAULTS = {
  title: {
    template:
      "[url=][color=white][b]{{game_name}} [{{os}}] [Branch: {{branch}}] (Clean Steam Files)[/b][/color][/url]",
  },
  version: {
    template:
      "[size=85][color=white][b]Version:[/b] [i]{{build_datetime_utc}} [Build {{build_id}}][/i][/color][/size]",
  },
  depot_list: {
    title: "\"[color=white]Depots & Manifests[/color]\"",
    lineTemplate: "{{depot_id}} - {{depot_name}} [Manifest {{manifest_id}}]",
    useCodeBlock: true,
  },
  free_text: {
    text: "",
  },
  uploaded_version: {
    template:
      "[color=white][b]Uploaded version:[/b] [i]{{build_datetime_utc}} [Build {{build_id}}][/i][/color]",
  },
  advertise_omnipacker: {
    // Fixed-content block; renders OMNIPACKER_CREDIT with no editable fields.
  },
};

const templateState = {
  blocks: [],
  metadata: null,
  // Name of the profile currently loaded in the editor scratchpad.
  editorProfileName: "standard",
  // True when the scratchpad has unsaved edits relative to the loaded profile.
  dirty: false,
};

let templateBlockSequence = 0;

const createTemplateBlockId = () => {
  if (typeof crypto !== "undefined" && crypto.randomUUID) {
    return crypto.randomUUID();
  }
  templateBlockSequence += 1;
  return `template-block-${Date.now()}-${templateBlockSequence}`;
};

const createTemplateBlock = (type) => ({
  id: createTemplateBlockId(),
  type,
  config: { ...(TEMPLATE_DEFAULTS[type] || {}) },
});

const createStandardTemplate = () => [
  createTemplateBlock("title"),
  createTemplateBlock("version"),
  createTemplateBlock("depot_list"),
  createTemplateBlock("uploaded_version"),
];

// Store-header body for the crew preset. `#UploadDate#` is an intentional
// literal placeholder the user edits by hand after uploading. Kept in sync with
// CREW_HEADER_FREETEXT in template_renderer.rs.
const CREW_HEADER_FREETEXT =
  "[img]https://steamcdn-a.akamaihd.net/steam/apps/{{app_id}}/header.jpg[/img]\n\n\n" +
  "[color=red][b]About This Game:[/b][/color]\n" +
  "[img]https://steamstore-a.akamaihd.net/public/images/v6/maincol_gradient_rule.png[/img]\n" +
  "{{game_description}}\n\n" +
  "[color=red][b]Official Site:[/b][/color]\n" +
  "[url]https://store.steampowered.com/app/{{app_id}}/[/url]\n" +
  "[url]{{website}}[/url]\n\n" +
  "[color=red][b]Download Links:[/b][/color]\n" +
  "[list][color=yellow][b]Mirror 1[/b][/color]\n" +
  "[url=][color=cyan]{{game_name}}[/color] | [color=#FF8000]#UploadDate#[/color][/url] " +
  "[i]< uploaded by {{username}} >[/i][/list]\n\n\n" +
  "[color=red][b]{{game_name}}[/b][/color]";

const createCrewTemplate = () => {
  const header = createTemplateBlock("free_text");
  header.config = { text: CREW_HEADER_FREETEXT };
  return [header, createTemplateBlock("depot_list"), createTemplateBlock("uploaded_version")];
};

// Built-in profile names. These are synthesized here, never written to disk,
// and cannot be overwritten or deleted (the backend rejects them too).
const BUILTIN_PROFILE_NAMES = ["standard", "crew"];

const isBuiltinProfileName = (name) =>
  BUILTIN_PROFILE_NAMES.some(
    (builtin) => builtin.toLowerCase() === String(name || "").trim().toLowerCase()
  );

// Display label for a profile. Built-ins are stored lowercase ("standard",
// "crew") so they map to reserved names everywhere; capitalize them for display
// only. Custom profiles keep their saved name as-is.
const profileDisplayName = (name) => {
  const value = String(name || "");
  if (isBuiltinProfileName(value)) {
    return value.charAt(0).toUpperCase() + value.slice(1).toLowerCase();
  }
  return value;
};

// Mirrors the backend sanitize_filename: keeps alphanumerics, dash, underscore;
// everything else becomes `_`; trims leading/trailing `_`; empty -> "profile".
// Used so a profile name maps to the same on-disk file the backend writes.
const sanitizeProfileFilename = (name) => {
  const sanitized = String(name || "").replace(/[^A-Za-z0-9_-]/g, "_");
  const trimmed = sanitized.replace(/^_+|_+$/g, "");
  return trimmed || "profile";
};

// Returns the built-in profile blocks for a given name (deep-cloned by the
// builders), or null if the name isn't a built-in.
const createBuiltinProfileBlocks = (name) => {
  const key = String(name || "").trim().toLowerCase();
  if (key === "standard") {
    return createStandardTemplate();
  }
  if (key === "crew") {
    return createCrewTemplate();
  }
  return null;
};

// The profile library: built-ins followed by saved custom profiles loaded from
// disk. Each entry: { name, builtin, payload }.
const profileState = {
  profiles: [],
};

const getBuiltinProfiles = () =>
  BUILTIN_PROFILE_NAMES.map((name) => ({
    name,
    builtin: true,
    payload: serializeTemplateBlocks(createBuiltinProfileBlocks(name)),
  }));

const findProfile = (name) =>
  profileState.profiles.find(
    (profile) => profile.name.toLowerCase() === String(name || "").trim().toLowerCase()
  ) || null;

// Loads saved profiles from the backend and rebuilds the library (built-ins +
// disk profiles). Falls back to built-ins only when the backend is unavailable.
const refreshProfileLibrary = async () => {
  let stored = [];
  if (tauriInvoke) {
    try {
      const result = await tauriInvoke("list_profiles");
      if (Array.isArray(result)) {
        stored = result
          .filter((entry) => entry && typeof entry.name === "string")
          .map((entry) => ({
            name: entry.name,
            builtin: false,
            payload: entry.payload,
          }));
      }
    } catch (error) {
      console.debug("[OmniPacker] Failed to list profiles:", error);
    }
  }
  profileState.profiles = [...getBuiltinProfiles(), ...stored];

  // Drop any generation selections whose profile no longer exists, but always
  // keep at least "standard" selected so a job still produces a template.
  const valid = new Set(profileState.profiles.map((p) => p.name));
  settingsState.selectedProfiles = settingsState.selectedProfiles.filter((name) =>
    valid.has(name)
  );
  if (settingsState.selectedProfiles.length === 0) {
    settingsState.selectedProfiles = ["standard"];
  }
  saveSettings();
};

const setTemplateStatus = (message) => {
  if (templateStatus) {
    templateStatus.textContent = message || "";
  }
};

const syncTemplatePreviewMeta = () => {
  if (!templatePreviewMeta) {
    return;
  }
  if (templateState.metadata) {
    templatePreviewMeta.textContent = t("template.preview.metaReady");
  } else {
    templatePreviewMeta.textContent = t("template.preview.metaDefault");
  }
};

const populateTemplateBlockSelect = () => {
  if (!templateBlockSelect) {
    return;
  }
  const currentValue = templateBlockSelect.value;
  templateBlockSelect.innerHTML = "";
  TEMPLATE_BLOCK_TYPES.forEach((blockType) => {
    const option = document.createElement("option");
    option.value = blockType.type;
    option.textContent = t(blockType.labelKey);
    templateBlockSelect.appendChild(option);
  });
  if (currentValue) {
    templateBlockSelect.value = currentValue;
  }
};

const loadTemplateMetadata = async () => {
  if (!tauriInvoke) {
    templateState.metadata = null;
    syncTemplatePreviewMeta();
    return;
  }
  try {
    const metadata = await tauriInvoke("get_template_metadata");
    templateState.metadata = metadata || null;
  } catch (error) {
    templateState.metadata = null;
  }
  syncTemplatePreviewMeta();
};

// Marks the editor scratchpad as modified relative to the loaded profile and
// reflects it in the unsaved notice. Edits are not persisted to disk until the
// user explicitly saves a profile.
const markScratchpadDirty = () => {
  templateState.dirty = true;
  syncUnsavedNotice();
};

const syncUnsavedNotice = () => {
  if (templateUnsavedNotice) {
    templateUnsavedNotice.hidden = !templateState.dirty;
  }
};

// Loads a profile's blocks into the editor scratchpad and marks it clean.
const loadProfileIntoEditor = (name) => {
  const profile = findProfile(name);
  let blocks;
  if (profile) {
    try {
      blocks = parseTemplatePayload(profile.payload);
    } catch (error) {
      blocks = createBuiltinProfileBlocks("standard");
    }
  } else {
    blocks = createBuiltinProfileBlocks("standard");
    name = "standard";
  }
  templateState.blocks = blocks;
  templateState.editorProfileName = profile ? profile.name : name;
  templateState.dirty = false;
  syncUnsavedNotice();
  renderTemplateBuilder();
  renderTemplatePreview();
  renderProfileOptions();
  updateProfileSummary();
};

// Guards a scratchpad-discarding action (switching the edited profile or
// closing the editor) with a confirm when there are unsaved changes. Returns
// true when it is safe to proceed.
const confirmDiscardScratchpad = async () => {
  if (!templateState.dirty) {
    return true;
  }
  return openGenericConfirm(
    t("template.unsavedConfirm.title"),
    t("template.unsavedConfirm.message")
  );
};

const openTemplateEditor = async () => {
  templateModalOverlay?.classList.add("active");
  await refreshProfileLibrary();
  // Load the first selected profile (or standard) into the editor on open.
  const initial =
    settingsState.selectedProfiles.find((name) => findProfile(name)) ||
    "standard";
  loadProfileIntoEditor(initial);
  populateTemplateBlockSelect();
  renderProfileOptions();
  updateProfileSummary();
  setTemplateStatus(t("template.status.ready"));
  await loadTemplateMetadata();
  renderTemplateBuilder();
  renderTemplatePreview();
};

const closeTemplateEditor = async () => {
  if (!(await confirmDiscardScratchpad())) {
    return;
  }
  templateState.dirty = false;
  syncUnsavedNotice();
  templateModalOverlay?.classList.remove("active");
};

const moveTemplateBlock = (index, direction) => {
  const nextIndex = index + direction;
  if (
    nextIndex < 0 ||
    nextIndex >= templateState.blocks.length ||
    index < 0
  ) {
    return;
  }
  const [block] = templateState.blocks.splice(index, 1);
  templateState.blocks.splice(nextIndex, 0, block);
  markScratchpadDirty();
  renderTemplateBuilder();
  renderTemplatePreview();
};

const removeTemplateBlock = (index) => {
  if (index < 0 || index >= templateState.blocks.length) {
    return;
  }
  templateState.blocks.splice(index, 1);
  markScratchpadDirty();
  renderTemplateBuilder();
  renderTemplatePreview();
};

const updateTemplateBlockConfig = (blockId, updates) => {
  const target = templateState.blocks.find((block) => block.id === blockId);
  if (!target) {
    return;
  }
  target.config = { ...target.config, ...updates };
  markScratchpadDirty();
  renderTemplatePreview();
};

const addTemplateBlock = (type) => {
  const selected = TEMPLATE_BLOCK_TYPES.find((block) => block.type === type);
  if (!selected) {
    return;
  }
  templateState.blocks.push(createTemplateBlock(type));
  markScratchpadDirty();
  renderTemplateBuilder();
  renderTemplatePreview();
};

// Inserts `{{field}}` into `input` at the caret (or appends to the end if the
// input isn't focused), then persists via the block's existing config key.
const insertTemplateToken = (input, blockId, configKey, field) => {
  const token = `{{${field}}}`;
  const focused = document.activeElement === input;
  const start = focused ? input.selectionStart : input.value.length;
  const end = focused ? input.selectionEnd : input.value.length;
  const next = input.value.slice(0, start) + token + input.value.slice(end);
  input.value = next;
  input.focus();
  const caret = start + token.length;
  input.setSelectionRange(caret, caret);
  updateTemplateBlockConfig(blockId, { [configKey]: next });
};

// Builds a row of click-to-insert token chips beneath a block's text input, one
// per field the block accepts. Iterating the field arrays keeps the chips in
// lockstep with the render engine (add a field there -> a chip appears here).
const appendTemplateChipRow = (parentEl, input, blockId, configKey, fields) => {
  const row = document.createElement("div");
  row.className = "template-chip-row";

  const hint = document.createElement("span");
  hint.className = "template-chip-hint";
  hint.textContent = t("template.help.chipHint");
  row.appendChild(hint);

  fields.forEach((field) => {
    const chip = document.createElement("button");
    chip.type = "button";
    chip.className = "template-chip";
    chip.textContent = `{{${field}}}`;
    const crewOnly = TEMPLATE_CREW_ONLY_FIELDS.has(field);
    if (crewOnly) {
      chip.classList.add("template-chip-crew");
    }
    const desc = t(templateFieldDescKey(field));
    chip.title = crewOnly ? `${desc} ${t("template.help.crewOnly")}` : desc;
    // mousedown + preventDefault keeps the text input focused so the caret
    // position survives the click (a plain click would steal focus first).
    chip.addEventListener("mousedown", (event) => {
      event.preventDefault();
      insertTemplateToken(input, blockId, configKey, field);
    });
    row.appendChild(chip);
  });

  parentEl.appendChild(row);
};

const renderTemplateBuilder = () => {
  if (!templateBlocksContainer) {
    return;
  }
  templateBlocksContainer.innerHTML = "";

  templateState.blocks.forEach((block, index) => {
    const blockEl = document.createElement("div");
    blockEl.className = "template-block";

    const header = document.createElement("div");
    header.className = "template-block-header";

    const title = document.createElement("div");
    title.textContent = t(`template.block.${block.type}`);

    const actions = document.createElement("div");
    actions.className = "template-block-actions";

    const upButton = document.createElement("button");
    upButton.type = "button";
    upButton.textContent = "Up";
    upButton.title = t("template.action.up");
    upButton.disabled = index === 0;
    upButton.addEventListener("click", () => moveTemplateBlock(index, -1));

    const downButton = document.createElement("button");
    downButton.type = "button";
    downButton.textContent = "Down";
    downButton.title = t("template.action.down");
    downButton.disabled = index === templateState.blocks.length - 1;
    downButton.addEventListener("click", () => moveTemplateBlock(index, 1));

    const removeButton = document.createElement("button");
    removeButton.type = "button";
    removeButton.textContent = "X";
    removeButton.title = t("template.action.remove");
    removeButton.addEventListener("click", () => removeTemplateBlock(index));

    actions.appendChild(upButton);
    actions.appendChild(downButton);
    actions.appendChild(removeButton);

    header.appendChild(title);
    header.appendChild(actions);
    blockEl.appendChild(header);

    if (block.type === "title" || block.type === "version" || block.type === "uploaded_version") {
      const field = document.createElement("div");
      field.className = "template-block-field";
      const label = document.createElement("label");
      label.textContent = t("template.field.template");
      const input = document.createElement("input");
      input.type = "text";
      input.value = block.config.template || "";
      input.addEventListener("input", () =>
        updateTemplateBlockConfig(block.id, { template: input.value })
      );
      field.appendChild(label);
      field.appendChild(input);
      appendTemplateChipRow(field, input, block.id, "template", TEMPLATE_SINGLE_FIELDS);
      blockEl.appendChild(field);
    }

    if (block.type === "free_text") {
      const field = document.createElement("div");
      field.className = "template-block-field";
      const label = document.createElement("label");
      label.textContent = t("template.field.text");
      const textarea = document.createElement("textarea");
      textarea.value = block.config.text || "";
      textarea.addEventListener("input", () =>
        updateTemplateBlockConfig(block.id, { text: textarea.value })
      );
      field.appendChild(label);
      field.appendChild(textarea);
      appendTemplateChipRow(field, textarea, block.id, "text", TEMPLATE_SINGLE_FIELDS);
      blockEl.appendChild(field);
    }

    if (block.type === "depot_list") {
      const titleField = document.createElement("div");
      titleField.className = "template-block-field";
      const titleLabel = document.createElement("label");
      titleLabel.textContent = t("template.field.depotTitle");
      const titleInput = document.createElement("input");
      titleInput.type = "text";
      titleInput.value = block.config.title || "";
      titleInput.addEventListener("input", () =>
        updateTemplateBlockConfig(block.id, { title: titleInput.value })
      );
      titleField.appendChild(titleLabel);
      titleField.appendChild(titleInput);
      blockEl.appendChild(titleField);

      const lineField = document.createElement("div");
      lineField.className = "template-block-field";
      const lineLabel = document.createElement("label");
      lineLabel.textContent = t("template.field.depotLine");
      const lineInput = document.createElement("input");
      lineInput.type = "text";
      lineInput.value = block.config.lineTemplate || "";
      lineInput.addEventListener("input", () =>
        updateTemplateBlockConfig(block.id, { lineTemplate: lineInput.value })
      );
      lineField.appendChild(lineLabel);
      lineField.appendChild(lineInput);
      appendTemplateChipRow(lineField, lineInput, block.id, "lineTemplate", TEMPLATE_DEPOT_FIELDS);
      blockEl.appendChild(lineField);

      const codeRow = document.createElement("label");
      codeRow.className = "checkbox-row";
      const codeToggle = document.createElement("input");
      codeToggle.type = "checkbox";
      codeToggle.checked = Boolean(block.config.useCodeBlock);
      codeToggle.addEventListener("change", () =>
        updateTemplateBlockConfig(block.id, { useCodeBlock: codeToggle.checked })
      );
      const codeLabel = document.createElement("span");
      codeLabel.textContent = t("template.field.useCode");
      codeRow.appendChild(codeToggle);
      codeRow.appendChild(codeLabel);
      blockEl.appendChild(codeRow);
    }

    templateBlocksContainer.appendChild(blockEl);
  });
};

const renderTemplateString = (template, allowedFields, values) => {
  const source = String(template ?? "");
  const tokenRegex = /\{\{([^}]+)\}\}/g;
  const tokens = [];
  let match = tokenRegex.exec(source);
  while (match) {
    tokens.push(match[1].trim());
    match = tokenRegex.exec(source);
  }

  const invalid = tokens.filter((token) => !allowedFields.includes(token));
  if (invalid.length) {
    const unique = [...new Set(invalid)];
    return { error: t("template.error.invalidField", { fields: unique.join(", ") }) };
  }

  const output = source.replace(tokenRegex, (_, token) => {
    const key = token.trim();
    if (Object.prototype.hasOwnProperty.call(values, key)) {
      return String(values[key] ?? "");
    }
    return "";
  });

  return { output };
};

const renderTemplateOutput = (blocks, metadata) => {
  if (!metadata) {
    return { error: t("template.error.noMetadata") };
  }

  const baseValues = {
    game_name: metadata.game_name || "",
    os: metadata.os || "",
    branch: metadata.branch || "",
    build_datetime_utc: metadata.build_datetime_utc || "",
    build_id: metadata.build_id || "",
    app_id: metadata.app_id || "",
    game_description: metadata.game_description || "",
    website: metadata.website || "",
    // The uploader handle comes from the global "Uploader name" setting and
    // feeds {{username}}. Blank when unset.
    username: settingsState.uploaderName || "",
  };
  const depots = Array.isArray(metadata.depots) ? metadata.depots : [];
  const outputParts = [];

  for (const block of blocks) {
    if (block.type === "title" || block.type === "version" || block.type === "uploaded_version") {
      const template = block.config.template || "";
      const rendered = renderTemplateString(template, TEMPLATE_SINGLE_FIELDS, baseValues);
      if (rendered.error) {
        return rendered;
      }
      outputParts.push(rendered.output);
      continue;
    }

    if (block.type === "free_text") {
      const template = block.config.text || "";
      const rendered = renderTemplateString(template, TEMPLATE_SINGLE_FIELDS, baseValues);
      if (rendered.error) {
        return rendered;
      }
      outputParts.push(rendered.output);
      continue;
    }

    if (block.type === "advertise_omnipacker") {
      outputParts.push(OMNIPACKER_CREDIT);
      continue;
    }

    if (block.type === "depot_list") {
      if (depots.length === 0) {
        return { error: t("template.error.noDepots") };
      }
      if (depots.length > TEMPLATE_MAX_DEPOTS) {
        return { error: t("template.error.depotLimit", { limit: TEMPLATE_MAX_DEPOTS }) };
      }

      const lineTemplate = block.config.lineTemplate || "";
      const lines = [];
      for (const depot of depots) {
        const depotValues = {
          depot_id: depot.depot_id || "",
          depot_name: depot.depot_name || "",
          manifest_id: depot.manifest_id || "",
        };
        const rendered = renderTemplateString(
          lineTemplate,
          TEMPLATE_DEPOT_FIELDS,
          depotValues
        );
        if (rendered.error) {
          return rendered;
        }
        lines.push(rendered.output);
      }

      const title = block.config.title || "Depots";
      const useCode = Boolean(block.config.useCodeBlock);
      let depotOutput = `[spoiler=${title}]\n`;
      if (useCode) {
        depotOutput += "[code=text]";
      }
      depotOutput += lines.join("\n");
      if (useCode) {
        depotOutput += "[/code]";
      }
      depotOutput += "\n[/spoiler]";
      outputParts.push(depotOutput);
    }
  }

  let output = "";
  for (let i = 0; i < outputParts.length; i += 1) {
    const current = outputParts[i];
    const next = outputParts[i + 1];
    const currentType = blocks[i]?.type;
    const nextType = blocks[i + 1]?.type;
    output += current;
    if (next !== undefined) {
      // Match CS.RIN-style spacing between default blocks.
      let separator = "\n";
      if (currentType === "version" && nextType === "depot_list") {
        separator = "\n\n";
      } else if (currentType === "depot_list" && nextType === "uploaded_version") {
        separator = "";
      }
      output += separator;
    }
  }
  if (output.length > TEMPLATE_MAX_LENGTH) {
    return { error: t("template.error.lengthLimit", { limit: TEMPLATE_MAX_LENGTH }) };
  }

  return { output };
};

const renderTemplatePreview = () => {
  if (!templatePreviewOutput) {
    return;
  }

  // Use fallback test metadata if no job metadata is available
  const metadata = templateState.metadata || TEMPLATE_DEFAULT_METADATA;

  const result = renderTemplateOutput(templateState.blocks, metadata);
  if (result.error) {
    templatePreviewOutput.value = result.error;
    templatePreviewOutput.classList.add("is-error");
    if (templateCopyButton) {
      templateCopyButton.disabled = true;
    }
    return;
  }

  templatePreviewOutput.value = result.output;
  templatePreviewOutput.classList.remove("is-error");
  if (templateCopyButton) {
    templateCopyButton.disabled = false;
  }
};

const serializeTemplateBlocks = (blocks) => ({
  version: 1,
  blocks: blocks.map((block) => ({
    type: block.type,
    config: block.config,
  })),
});

const serializeTemplate = () => serializeTemplateBlocks(templateState.blocks);

const parseTemplatePayload = (payload) => {
  if (!payload || !Array.isArray(payload.blocks)) {
    throw new Error(t("template.error.invalidFile"));
  }
  const parsedBlocks = payload.blocks.map((block) => {
    if (!block || typeof block.type !== "string") {
      throw new Error(t("template.error.invalidFile"));
    }
    if (!TEMPLATE_DEFAULTS[block.type]) {
      throw new Error(t("template.error.invalidFile"));
    }
    const config = block.config && typeof block.config === "object" ? block.config : {};
    let sanitized = {};
    if (block.type === "title" || block.type === "version" || block.type === "uploaded_version") {
      sanitized = {
        template:
          typeof config.template === "string"
            ? config.template
            : TEMPLATE_DEFAULTS[block.type].template,
      };
    } else if (block.type === "free_text") {
      sanitized = {
        text:
          typeof config.text === "string"
            ? config.text
            : TEMPLATE_DEFAULTS.free_text.text,
      };
    } else if (block.type === "advertise_omnipacker") {
      sanitized = {};
    } else if (block.type === "depot_list") {
      sanitized = {
        title:
          typeof config.title === "string"
            ? config.title
            : TEMPLATE_DEFAULTS.depot_list.title,
        lineTemplate:
          typeof config.lineTemplate === "string"
            ? config.lineTemplate
            : TEMPLATE_DEFAULTS.depot_list.lineTemplate,
        useCodeBlock:
          typeof config.useCodeBlock === "boolean"
            ? config.useCodeBlock
            : TEMPLATE_DEFAULTS.depot_list.useCodeBlock,
      };
    }
    return {
      id: createTemplateBlockId(),
      type: block.type,
      config: sanitized,
    };
  });
  if (parsedBlocks.length === 0) {
    throw new Error(t("template.error.invalidFile"));
  }
  return parsedBlocks;
};

const isValidTemplatePayload = (payload) => {
  if (!payload || typeof payload !== "object") {
    return false;
  }
  try {
    parseTemplatePayload(payload);
    return true;
  } catch {
    return false;
  }
};

// Generic yes/no confirm backed by the template-generic-confirm modal. Resolves
// true on Yes, false on No/dismiss. Falls back to window.confirm when the modal
// isn't present (e.g. tests).
let genericConfirmResolve = null;

const openGenericConfirm = (title, message) => {
  if (!templateGenericConfirmOverlay) {
    return Promise.resolve(window.confirm(message));
  }
  if (templateGenericConfirmTitle) {
    templateGenericConfirmTitle.textContent = title || "";
  }
  if (templateGenericConfirmMessage) {
    templateGenericConfirmMessage.textContent = message || "";
  }
  if (genericConfirmResolve) {
    genericConfirmResolve(false);
    genericConfirmResolve = null;
  }
  templateGenericConfirmOverlay.classList.add("active");
  return new Promise((resolve) => {
    genericConfirmResolve = resolve;
  });
};

const closeGenericConfirm = (result) => {
  templateGenericConfirmOverlay?.classList.remove("active");
  if (genericConfirmResolve) {
    genericConfirmResolve(result);
    genericConfirmResolve = null;
  }
};

// Persists the current scratchpad blocks as a named profile via the backend,
// then refreshes the library and selects the saved profile for editing.
const persistProfile = async (name) => {
  if (!tauriInvoke) {
    setTemplateStatus(t("template.error.saveFailed", { error: "backend unavailable" }));
    return false;
  }
  try {
    const savedName = await tauriInvoke("save_profile", {
      name,
      templatePayload: serializeTemplate(),
    });
    await refreshProfileLibrary();
    const canonical = typeof savedName === "string" && savedName ? savedName : name;
    templateState.editorProfileName = canonical;
    templateState.dirty = false;
    syncUnsavedNotice();
    // Newly saved profiles are auto-selected for generation.
    if (!settingsState.selectedProfiles.includes(canonical)) {
      settingsState.selectedProfiles.push(canonical);
      saveSettings();
    }
    renderProfileOptions();
    updateProfileSummary();
    setTemplateStatus(t("template.status.saved"));
    return true;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setTemplateStatus(t("template.error.saveFailed", { error: message }));
    return false;
  }
};

// Opens the save-profile modal, pre-filling the current editor profile name
// (unless it's a built-in, which can't be overwritten).
const openSaveProfileModal = () => {
  if (!templateSaveOverlay || !templateSaveNameInput) {
    return;
  }
  const current = templateState.editorProfileName || "";
  templateSaveNameInput.value = isBuiltinProfileName(current) ? "" : current;
  if (templateSaveError) {
    templateSaveError.textContent = "";
  }
  templateSaveOverlay.classList.add("active");
  templateSaveNameInput.focus();
  templateSaveNameInput.select();
};

const closeSaveProfileModal = () => {
  templateSaveOverlay?.classList.remove("active");
};

// Validates the entered name and saves (prompting to overwrite when a custom
// profile of the same name exists). Reserved built-in names are rejected.
const confirmSaveProfile = async () => {
  const name = (templateSaveNameInput?.value || "").trim();
  if (!name) {
    if (templateSaveError) {
      templateSaveError.textContent = t("template.save.errorEmpty");
    }
    return;
  }
  if (isBuiltinProfileName(name)) {
    if (templateSaveError) {
      templateSaveError.textContent = t("template.save.errorReserved");
    }
    return;
  }
  if (findProfile(name)) {
    const overwrite = await openGenericConfirm(
      t("template.overwrite.title"),
      t("template.overwrite.message", { name })
    );
    if (!overwrite) {
      return;
    }
  }
  const ok = await persistProfile(name);
  if (ok) {
    closeSaveProfileModal();
  }
};

// Deletes a saved profile after confirmation. Built-ins cannot be deleted.
const deleteProfile = async (name) => {
  if (isBuiltinProfileName(name) || !tauriInvoke) {
    return;
  }
  const confirmed = await openGenericConfirm(
    t("template.delete.title"),
    t("template.delete.message", { name })
  );
  if (!confirmed) {
    return;
  }
  try {
    await tauriInvoke("delete_profile", { name });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setTemplateStatus(t("template.error.deleteFailed", { error: message }));
    return;
  }
  await refreshProfileLibrary();
  // If the deleted profile was loaded in the editor, fall back to standard.
  if (
    templateState.editorProfileName.toLowerCase() === name.toLowerCase()
  ) {
    templateState.dirty = false;
    loadProfileIntoEditor("standard");
  }
  renderProfileOptions();
  updateProfileSummary();
  setTemplateStatus(t("template.status.deleted"));
};

// Imports a shared profile JSON file into the library. Prompts for a name
// (defaulting to the file's base name) and saves it like any custom profile.
const importProfileFromFile = (file) => {
  const reader = new FileReader();
  reader.onload = async () => {
    let blocks;
    try {
      const payload = JSON.parse(String(reader.result || ""));
      blocks = parseTemplatePayload(payload);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setTemplateStatus(t("template.error.loadFailed", { error: message }));
      return;
    }
    // Load the imported blocks into the scratchpad and prompt to name/save.
    templateState.blocks = blocks;
    templateState.dirty = true;
    syncUnsavedNotice();
    renderTemplateBuilder();
    renderTemplatePreview();
    setTemplateStatus(t("template.status.imported"));
    const baseName = (file.name || "imported").replace(/\.json$/i, "");
    if (templateSaveNameInput) {
      openSaveProfileModal();
      templateSaveNameInput.value = isBuiltinProfileName(baseName) ? "" : baseName;
    }
  };
  reader.onerror = () => {
    const message =
      reader.error && reader.error.message ? reader.error.message : "unknown";
    setTemplateStatus(t("template.error.loadFailed", { error: message }));
  };
  reader.readAsText(file);
};

const openProfilesFolder = async () => {
  if (!tauriInvoke) {
    return;
  }
  try {
    await tauriInvoke("open_profiles_folder");
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    setTemplateStatus(t("template.error.openFolderFailed", { error: message }));
  }
};

const copyTemplatePreview = async () => {
  if (!templatePreviewOutput || templatePreviewOutput.classList.contains("is-error")) {
    return;
  }
  const text = templatePreviewOutput.value;
  if (!text) {
    return;
  }
  if (navigator?.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const textarea = document.createElement("textarea");
  textarea.value = text;
  document.body.appendChild(textarea);
  textarea.select();
  document.execCommand("copy");
  document.body.removeChild(textarea);
};

const refreshQrModalText = () => {
  if (!qrModalOutput) {
    return;
  }
  if (qrWaitingTextValues.includes(qrModalOutput.textContent)) {
    updateQrModalText(t("qr.waiting"));
  }
};

const applyTranslations = () => {
  if (document?.documentElement) {
    document.documentElement.lang = settingsState.language || "en";
  }
  document.querySelectorAll("[data-i18n]").forEach((element) => {
    const key = element.dataset.i18n;
    if (key) {
      element.textContent = t(key);
    }
  });
  document.querySelectorAll("[data-i18n-placeholder]").forEach((element) => {
    const key = element.dataset.i18nPlaceholder;
    if (key) {
      element.placeholder = t(key);
    }
  });
  if (typeof updateOsSummary === "function") {
    updateOsSummary();
  }
  if (typeof updateBranchSummary === "function") {
    renderBranchOptions();
    updateBranchSummary();
  }
  refreshQrModalText();
  if (templateModalOverlay?.classList.contains("active")) {
    populateTemplateBlockSelect();
    renderProfileOptions();
    updateProfileSummary();
    renderTemplateBuilder();
    syncTemplatePreviewMeta();
    renderTemplatePreview();
  }
  if (outputConflictOverlay?.classList.contains("active")) {
    updateOutputConflictText();
  }
};

// Load settings from localStorage on startup
const loadSettings = () => {
  try {
    const saved = localStorage.getItem("omnipacker-settings");
    if (saved) {
      const parsed = JSON.parse(saved);
      if (typeof parsed.skipCompression === "boolean") {
        settingsState.skipCompression = parsed.skipCompression;
      }
      if (typeof parsed.compressionPasswordEnabled === "boolean") {
        settingsState.compressionPasswordEnabled =
          parsed.compressionPasswordEnabled;
      }
      if (typeof parsed.compressionPassword === "string") {
        settingsState.compressionPassword = parsed.compressionPassword;
      }
      if (typeof parsed.customCompressionArgs === "string") {
        settingsState.customCompressionArgs = parsed.customCompressionArgs;
      }
      if (typeof parsed.splitArchiveEnabled === "boolean") {
        settingsState.splitArchiveEnabled = parsed.splitArchiveEnabled;
      }
      if (typeof parsed.splitSizePreset === "string") {
        settingsState.splitSizePreset = parsed.splitSizePreset;
      }
      if (typeof parsed.splitCustomSize === "string") {
        settingsState.splitCustomSize = parsed.splitCustomSize;
      }
      if (parsed.splitCustomUnit === "m" || parsed.splitCustomUnit === "g") {
        settingsState.splitCustomUnit = parsed.splitCustomUnit;
      }
      if (typeof parsed.defaultQrLogin === "boolean") {
        settingsState.defaultQrLogin = parsed.defaultQrLogin;
      }
      if (typeof parsed.language === "string") {
        settingsState.language = parsed.language;
      }
      if (typeof parsed.skippedUpdateVersion === "string") {
        settingsState.skippedUpdateVersion = parsed.skippedUpdateVersion;
      }
      if (typeof parsed.uploaderName === "string") {
        settingsState.uploaderName = parsed.uploaderName;
      }
      if (
        Array.isArray(parsed.selectedProfiles) &&
        parsed.selectedProfiles.every((name) => typeof name === "string")
      ) {
        settingsState.selectedProfiles = parsed.selectedProfiles;
      }
    }
    if (
      settingsState.compressionPasswordEnabled &&
      !settingsState.compressionPassword.trim()
    ) {
      settingsState.compressionPasswordEnabled = false;
    }
    if (
      settingsState.splitArchiveEnabled &&
      settingsState.splitSizePreset === "custom" &&
      !resolveSplitVolumeSize()
    ) {
      settingsState.splitArchiveEnabled = false;
    }
  } catch (e) {
    console.debug("[OmniPacker] Failed to load settings:", e);
  }
};

// Save settings to localStorage
const saveSettings = () => {
  try {
    localStorage.setItem("omnipacker-settings", JSON.stringify(settingsState));
  } catch (e) {
    console.debug("[OmniPacker] Failed to save settings:", e);
  }
};

// Resolves the current split settings into a 7-Zip volume-size token like
// "100m" or "4g". Returns "" when splitting is off or the custom size is
// missing/invalid, so callers can treat empty as "no split".
const resolveSplitVolumeSize = () => {
  if (!settingsState.splitArchiveEnabled) {
    return "";
  }
  if (settingsState.splitSizePreset === "custom") {
    const value = parseInt(settingsState.splitCustomSize, 10);
    if (!Number.isFinite(value) || value <= 0) {
      return "";
    }
    const unit = settingsState.splitCustomUnit === "g" ? "g" : "m";
    return `${value}${unit}`;
  }
  return settingsState.splitSizePreset;
};

const syncSplitArchiveUI = () => {
  if (!splitArchiveToggle || !splitSizePreset) {
    return;
  }
  const enabled = splitArchiveToggle.checked;
  const isCustom = splitSizePreset.value === "custom";
  splitSizePreset.disabled = !enabled;
  if (splitCustomSize) {
    splitCustomSize.disabled = !enabled || !isCustom;
  }
  if (splitCustomUnit) {
    splitCustomUnit.disabled = !enabled || !isCustom;
  }
  if (splitCustomRow) {
    splitCustomRow.classList.toggle("active", isCustom);
  }
};

const syncCompressionPasswordUI = (shouldFocus = false) => {
  if (!compressionPasswordToggle || !compressionPasswordInput) {
    return;
  }
  const enabled = compressionPasswordToggle.checked;
  compressionPasswordInput.disabled = !enabled;
  if (enabled && shouldFocus) {
    compressionPasswordInput.focus();
  }
};

const isCompressionPasswordValid = () => {
  if (!compressionPasswordToggle?.checked) {
    return true;
  }
  return Boolean(compressionPasswordInput?.value?.trim());
};

// Apply settings to UI
const applySettingsToUI = () => {
  if (skipCompressionToggle) {
    skipCompressionToggle.checked = settingsState.skipCompression;
  }
  if (compressionPasswordToggle) {
    compressionPasswordToggle.checked = settingsState.compressionPasswordEnabled;
  }
  if (compressionPasswordInput) {
    compressionPasswordInput.value = settingsState.compressionPassword;
  }
  if (customCompressionArgsInput) {
    customCompressionArgsInput.value = settingsState.customCompressionArgs;
  }
  if (splitArchiveToggle) {
    splitArchiveToggle.checked = settingsState.splitArchiveEnabled;
  }
  if (splitSizePreset) {
    splitSizePreset.value = settingsState.splitSizePreset;
  }
  if (splitCustomSize) {
    splitCustomSize.value = settingsState.splitCustomSize;
  }
  if (splitCustomUnit) {
    splitCustomUnit.value = settingsState.splitCustomUnit;
  }
  if (defaultQrToggle) {
    defaultQrToggle.checked = settingsState.defaultQrLogin;
  }
  if (languageSelect) {
    languageSelect.value = settingsState.language;
  }
  if (uploaderNameInput) {
    uploaderNameInput.value = settingsState.uploaderName || "";
  }
  syncCompressionPasswordUI();
  syncSplitArchiveUI();
  applyTranslations();
};

const applyDefaultQrLogin = () => {
  if (!qrLoginToggle) {
    return;
  }
  qrLoginToggle.checked = Boolean(settingsState.defaultQrLogin);
  const credentialsPanel = qrLoginToggle.closest(".credentials");
  credentialsPanel?.classList.toggle("qr-enabled", qrLoginToggle.checked);
  renderAll();
};

let jobSequence = 0;

const createJobId = () => {
  if (typeof crypto !== "undefined" && crypto.randomUUID) {
    return crypto.randomUUID();
  }
  jobSequence += 1;
  return `job-${Date.now()}-${jobSequence}`;
};

// Shared credentials + appId for a queueing action. OS and branch are resolved
// separately (see getSelectedOsValues / getSelectedBranches) so one click can
// fan out into the cartesian product of checked OS × checked branches.
const getCredentialSnapshot = () => ({
  appId: appIdInput?.value?.trim() || "unknown",
  username: steamUsernameInput?.value?.trim() || "",
  password: steamPasswordInput?.value || "",
  qrEnabled: Boolean(qrLoginToggle?.checked),
});

const syncAuthFromForm = (job) => {
  if (!job) {
    return;
  }
  job.username = steamUsernameInput?.value?.trim() || "";
  job.password = steamPasswordInput?.value || "";
  job.qrEnabled = Boolean(qrLoginToggle?.checked);
  job.rememberPassword = false;
};

const extractRememberedUsername = (line) => {
  if (!line) {
    return null;
  }
  const match = line.match(/-username\s+(\S+)/);
  return match ? match[1] : null;
};

const rememberQrLogin = (job, line) => {
  if (!job?.qrEnabled) {
    return;
  }
  const username = extractRememberedUsername(line);
  if (!username) {
    return;
  }
  authState.rememberedUsername = username;

  // Update current job so subsequent operations can reuse credentials
  job.username = username;
  job.rememberPassword = true;
};

const applyRememberedAuth = (job) => {
  if (!job) {
    return;
  }
  if (job.qrEnabled && authState.rememberedUsername) {
    job.qrEnabled = false;
    job.username = authState.rememberedUsername;
    job.password = "";
    job.rememberPassword = true;
    pushJobLog(job, t("auth.reuseQr", { username: authState.rememberedUsername }));
  }
};

const setSavedLogin = (login) => {
  authState.savedLogin = login;
  if (steamUsernameInput) {
    steamUsernameInput.value = login?.username ?? "";
  }
  if (steamPasswordInput) {
    steamPasswordInput.value = login?.password ?? "";
  }
};

const hasSavedLogin = () => Boolean(authState.savedLogin);

const createJob = ({ appId, os, branch, branchPassword, username, password, qrEnabled }) => {
  const job = {
    id: createJobId(),
    appId,
    os,
    branch,
    branchPassword: branchPassword || "",
    username,
    password,
    qrEnabled,
    rememberPassword: false,
    status: "queued",
    logs: [],
    compressionProgress: null,
    qrText: null,
    qrCaptureActive: false,
    qrCaptureLines: [],
    steamGuardEmailPending: false,
    backendJobId: null, // Job ID assigned by backend (staging directory name)
    stagingDir: null, // Staging directory path
  };
  jobState.jobs.set(job.id, job);
  jobState.order.push(job.id);
  return job;
};

const getRunningJob = () =>
  jobState.runningJobId
    ? jobState.jobs.get(jobState.runningJobId) || null
    : null;

const getSelectedJob = () =>
  jobState.selectedJobId
    ? jobState.jobs.get(jobState.selectedJobId) || null
    : null;

const getJobByBackendId = (backendJobId) => {
  if (!backendJobId) {
    return null;
  }
  for (const job of jobState.jobs.values()) {
    if (job.backendJobId === backendJobId) {
      return job;
    }
  }
  return null;
};

const resolveEventJob = (payload) => {
  const backendJobId = payload?.jobId;
  if (backendJobId) {
    const matchingJob = getJobByBackendId(backendJobId);
    if (matchingJob) {
      return matchingJob;
    }
    // Startup race: the backend emits the first events carrying the real
    // job_id before run_depotdownloader has returned it to the frontend, so
    // the running job may not have its backendJobId set yet. Bind it here.
    const runningJob = getRunningJob();
    if (runningJob && !runningJob.backendJobId) {
      runningJob.backendJobId = backendJobId;
      return runningJob;
    }
    // A jobId that matches no job while the running job is already identified
    // is a stale/stray event. Orphan it instead of misattributing it to the
    // current job (which would pollute its log or drive a bogus transition).
    return null;
  }
  // jobId-less events still attach to the running job.
  return getRunningJob();
};

const getNextQueuedJob = () => {
  for (const jobId of jobState.order) {
    const job = jobState.jobs.get(jobId);
    if (job?.status === "queued") {
      return job;
    }
  }
  return null;
};

const hasQueuedJobs = () =>
  jobState.order.some((jobId) => jobState.jobs.get(jobId)?.status === "queued");

// True when `job` is the only job still queued, i.e. the final job of the
// current queue run. Used to decide when DepotDownloader should wipe its stored
// login token (see clearToken in buildJobMetadata).
const isLastQueuedJob = (job) =>
  !jobState.order.some((jobId) => {
    const other = jobState.jobs.get(jobId);
    return other && other !== job && other.status === "queued";
  });

const isRunningJob = (jobId) => jobState.runningJobId === jobId;
const isQueueRunning = () => Boolean(jobState.runningJobId);

// Adding requires at least one OS and one branch checked, since one click fans
// out into the cartesian product of the two. Also blocked while a job runs.
const syncAddToQueueButton = () => {
  if (!addToQueueButton) {
    return;
  }
  const hasSelection =
    getSelectedOsValues().length > 0 && getSelectedBranches().length > 0;
  addToQueueButton.disabled = isQueueRunning() || !hasSelection;
};

const updateFormInputState = () => {
  const running = isQueueRunning();
  const qrEnabled = Boolean(qrLoginToggle?.checked);
  const loginLocked = hasSavedLogin();

  // Disable form inputs
  if (appIdInput) appIdInput.disabled = running;
  setDropdownDisabled(osDropdown, running);
  setDropdownDisabled(branchDropdown, running);
  syncAddToQueueButton();

  // Disable credentials (QR or saved login lock)
  if (steamUsernameInput) {
    steamUsernameInput.disabled = running || qrEnabled || loginLocked;
  }
  if (steamPasswordInput) {
    steamPasswordInput.disabled = running || qrEnabled || loginLocked;
  }

  // Visual feedback
  const gameManager = document.querySelector(".game-manager");
  const branchManager = document.querySelector('[aria-label="Branch Manager"]');
  const credentials = document.querySelector(".credentials");

  if (gameManager) gameManager.classList.toggle("form-inputs-disabled", running);
  if (branchManager) branchManager.classList.toggle("form-inputs-disabled", running);
  if (credentials) credentials.classList.toggle("form-inputs-disabled", running);
};

const moveJob = (jobId, direction) => {
  if (isQueueRunning()) {
    return;
  }
  if (isRunningJob(jobId)) {
    return;
  }
  const index = jobState.order.indexOf(jobId);
  if (index === -1) {
    return;
  }
  const nextIndex = index + direction;
  if (nextIndex < 0 || nextIndex >= jobState.order.length) {
    return;
  }
  const order = jobState.order;
  [order[index], order[nextIndex]] = [order[nextIndex], order[index]];
};

const removeJob = (jobId) => {
  if (isQueueRunning()) {
    return;
  }
  if (isRunningJob(jobId)) {
    return;
  }
  const index = jobState.order.indexOf(jobId);
  if (index === -1) {
    return;
  }
  jobState.order.splice(index, 1);
  jobState.jobs.delete(jobId);

  if (jobState.selectedJobId === jobId) {
    if (jobState.order.length === 0) {
      jobState.selectedJobId = null;
      return;
    }
    const nextIndex = Math.min(index, jobState.order.length - 1);
    jobState.selectedJobId = jobState.order[nextIndex] ?? null;
  }
};

const clearQueuedJobs = () => {
  if (isQueueRunning()) {
    alert(t("alert.clearQueue"));
    return;
  }

  if (jobState.order.length === 0) {
    return;
  }

  // Clear all jobs
  jobState.jobs.clear();
  jobState.order = [];
  jobState.selectedJobId = null;
};

const trimJobLogs = (job) => {
  if (job.logs.length <= LOG_LINE_CAP + LOG_TRIM_MARGIN) {
    return false;
  }
  const trimCount = job.logs.length - LOG_LINE_CAP;
  job.logs.splice(0, trimCount);
  return true;
};

const scheduleConsoleRender = () => {
  if (!consoleOutput || activeTab !== "console") {
    return;
  }
  if (consoleRenderState.timer) {
    return;
  }
  consoleRenderState.timer = window.setTimeout(() => {
    consoleRenderState.timer = null;
    renderConsole();
  }, CONSOLE_FLUSH_INTERVAL_MS);
};

const debugConsoleLog = (message) => {
  if (!tauriInvoke) {
    return;
  }
  void tauriInvoke("debug_console_log", { line: message }).catch(() => {});
};

const pushJobLog = (job, message, options = {}) => {
  if (!job) {
    return;
  }
  const { forwardToDebug = true } = options;
  job.logs.push(message);
  const trimmed = trimJobLogs(job);
  if (forwardToDebug) {
    debugConsoleLog(message);
  }
  if (job.id === jobState.selectedJobId) {
    if (trimmed) {
      consoleRenderState.needsFullRender = true;
    }
    scheduleConsoleRender();
  }
};

const appendLogToJob = (job, payload) => {
  const line = payload?.line ?? "";
  const stream = payload?.stream ? `[${payload.stream}] ` : "";
  pushJobLog(job, `${stream}${line}`, { forwardToDebug: false });
};

const appendSystemMessage = (message) => {
  const selectedJob = getSelectedJob();
  if (selectedJob) {
    pushJobLog(selectedJob, message);
    renderAll();
    return;
  }
  if (consoleOutput) {
    consoleOutput.value = message;
    debugConsoleLog(message);
    return;
  }
  debugConsoleLog(message);
  console.debug(`[OmniPacker] ${message}`);
};

const updateQrModalText = (text) => {
  if (!qrModalOutput) {
    return;
  }
  qrModalOutput.textContent = text;
};

const openQrModal = () => {
  qrModalOverlay?.classList.add("active");
};

const closeQrModal = () => {
  qrModalOverlay?.classList.remove("active");
};

let appVersionLoaded = false;
const loadAppVersion = async () => {
  if (appVersionLoaded || !settingsVersionLabel || !tauriInvoke) {
    return;
  }
  try {
    const version = await tauriInvoke("get_app_version");
    settingsVersionLabel.textContent = version ? `v${version}` : "";
    appVersionLoaded = true;
  } catch {
    settingsVersionLabel.textContent = "";
  }
};

const openSettingsModal = () => {
  applySettingsToUI();
  void loadAppVersion();
  settingsModalOverlay?.classList.add("active");
};

const closeSettingsModal = () => {
  if (!isCompressionPasswordValid()) {
    window.alert(t("settings.compressionPasswordRequired"));
    compressionPasswordInput?.focus();
    return;
  }
  settingsModalOverlay?.classList.remove("active");
};

const openSteamGuardModal = () => {
  steamGuardModalOverlay?.classList.add("active");
};

const closeSteamGuardModal = () => {
  steamGuardModalOverlay?.classList.remove("active");
};

const setSteamGuardEmailBusy = (busy) => {
  steamGuardEmailState.busy = busy;
  if (steamGuardEmailInput) {
    steamGuardEmailInput.disabled = busy;
  }
  if (steamGuardEmailSubmitButton) {
    steamGuardEmailSubmitButton.disabled = busy;
  }
};

const updateSteamGuardEmailMessage = (line) => {
  if (!steamGuardEmailMessage) {
    return;
  }
  const provider = extractSteamGuardEmailProvider(line);
  steamGuardEmailState.provider = provider;
  steamGuardEmailMessage.textContent = `STEAM GUARD! Please enter the auth code sent to the email at ${provider}`;
};

const openSteamGuardEmailModal = (line, jobId) => {
  steamGuardEmailState.jobId = jobId || null;
  setSteamGuardEmailBusy(false);
  updateSteamGuardEmailMessage(line);
  if (steamGuardEmailInput) {
    steamGuardEmailInput.value = "";
  }
  if (steamGuardEmailStatus) {
    steamGuardEmailStatus.textContent = t("steamGuard.email.status");
  }
  steamGuardEmailOverlay?.classList.add("active");
  steamGuardEmailInput?.focus();
};

const closeSteamGuardEmailModal = () => {
  const job = steamGuardEmailState.jobId
    ? jobState.jobs.get(steamGuardEmailState.jobId)
    : null;
  if (job) {
    job.steamGuardEmailPending = false;
  }
  steamGuardEmailOverlay?.classList.remove("active");
  setSteamGuardEmailBusy(false);
  steamGuardEmailState.jobId = null;
  steamGuardEmailState.provider = "";
  if (steamGuardEmailInput) {
    steamGuardEmailInput.value = "";
  }
};

const resetJobForRetry = (job) => {
  if (!job) {
    return;
  }
  job.status = "queued";
  job.backendJobId = null;
  job.stagingDir = null;
  job.compressionProgress = null;
  job.qrText = null;
  job.qrCaptureActive = false;
  job.qrCaptureLines = [];
  job.steamGuardPending = false;
  job.steamGuardEmailPending = false;
};

const requestSteamGuardEmailRetry = (job) => {
  if (!job || steamGuardEmailRetryState.requested) {
    return;
  }
  steamGuardEmailRetryState.requested = true;
  steamGuardEmailRetryState.jobId = job.id;
  job.steamGuardEmailPending = false;
  pushJobLog(job, t("steamGuard.email.incorrect"));
  closeSteamGuardEmailModal();
  renderAll();

  if (!tauriInvoke) {
    pushJobLog(job, t("auth.tauriUnavailable"));
    steamGuardEmailRetryState.requested = false;
    steamGuardEmailRetryState.jobId = null;
    return;
  }

  void tauriInvoke("cancel_depotdownloader").catch((error) => {
    pushJobLog(job, t("job.cancelFailed", { error }));
    steamGuardEmailRetryState.requested = false;
    steamGuardEmailRetryState.jobId = null;
    renderAll();
  });
};

const submitSteamGuardEmailCode = async () => {
  const code = steamGuardEmailInput?.value?.trim() ?? "";
  if (!code) {
    if (steamGuardEmailStatus) {
      steamGuardEmailStatus.textContent = t("steamGuard.email.empty");
    }
    return;
  }

  if (!tauriInvoke) {
    if (steamGuardEmailStatus) {
      steamGuardEmailStatus.textContent = t("auth.tauriUnavailable");
    }
    return;
  }

  const job = steamGuardEmailState.jobId
    ? jobState.jobs.get(steamGuardEmailState.jobId)
    : getRunningJob();

  try {
    setSteamGuardEmailBusy(true);
    await tauriInvoke("submit_steam_guard_code", { code });
    if (job) {
      pushJobLog(job, t("steamGuard.email.sent"));
      job.steamGuardEmailPending = false;
    }
    closeSteamGuardEmailModal();
  } catch (error) {
    if (job) {
      pushJobLog(job, t("steamGuard.email.failed", { error }));
    }
    if (steamGuardEmailStatus) {
      steamGuardEmailStatus.textContent = t("steamGuard.email.failed", { error });
    }
    setSteamGuardEmailBusy(false);
  }
};

const updateOutputConflictText = () => {
  if (!outputConflictMessage || !outputConflictPath) {
    return;
  }
  const name =
    outputConflictState.outputName ||
    outputConflictState.outputPath ||
    t("output.conflict.title");
  outputConflictMessage.textContent = t("output.conflict.message", { name });
  outputConflictPath.textContent = t("output.conflict.path", {
    path: outputConflictState.outputPath,
  });
};

const setOutputConflictBusy = (busy) => {
  outputConflictState.busy = busy;
  if (outputConflictOverwriteButton) {
    outputConflictOverwriteButton.disabled = busy;
  }
  if (outputConflictCopyButton) {
    outputConflictCopyButton.disabled = busy;
  }
  if (outputConflictCancelButton) {
    outputConflictCancelButton.disabled = busy;
  }
};

const openOutputConflictModal = (payload) => {
  outputConflictState.jobId = payload?.jobId ?? null;
  outputConflictState.outputName = payload?.outputName ?? "";
  outputConflictState.outputPath = payload?.outputPath ?? "";
  setOutputConflictBusy(false);
  updateOutputConflictText();
  outputConflictOverlay?.classList.add("active");
};

const closeOutputConflictModal = () => {
  outputConflictOverlay?.classList.remove("active");
  setOutputConflictBusy(false);
  outputConflictState.jobId = null;
  outputConflictState.outputName = "";
  outputConflictState.outputPath = "";
};

const sendOutputConflictChoice = async (choice) => {
  const jobId = outputConflictState.jobId;
  if (!jobId) {
    closeOutputConflictModal();
    return;
  }

  const job = getJobByBackendId(jobId) || getRunningJob();

  if (!tauriInvoke) {
    if (job) {
      pushJobLog(
        job,
        t("output.conflict.resolveError", {
          error: t("auth.tauriUnavailable"),
        }),
      );
      renderAll();
    }
    closeOutputConflictModal();
    return;
  }

  try {
    setOutputConflictBusy(true);
    await tauriInvoke("resolve_output_conflict", { jobId, choice });
    if (job) {
      pushJobLog(job, t(`output.conflict.choice.${choice}`));
      renderAll();
    }
  } catch (error) {
    if (job) {
      pushJobLog(job, t("output.conflict.resolveError", { error }));
      renderAll();
    }
  }

  closeOutputConflictModal();
};

// Detects Steam Guard mobile app confirmation prompt
const steamGuardPromptMatches = (line) => {
  return line.includes("STEAM GUARD!") && line.includes("Steam Mobile App");
};

const steamGuardEmailPromptMatches = (line) => {
  const lower = line.toLowerCase();
  return (
    lower.includes("steam guard") &&
    lower.includes("auth code sent to the email at")
  );
};

const steamGuardEmailFailureMatches = (line) =>
  line.includes("No code was provided by the authenticator");

const steamGuardEmailInvalidCodeMatches = (line) => {
  const lower = line.toLowerCase();
  return lower.includes(
    "previous 2-factor auth code you have provided is incorrect",
  );
};

const extractSteamGuardEmailProvider = (line) => {
  const match = line.match(/email at ([^:]+):?/i);
  return match ? match[1].trim() : "email";
};

// Detects successful login confirmation after Steam Guard
const steamGuardConfirmedMatches = (line) => {
  return line.trim() === "Done!" || line.includes("[stdout]  Done!");
};

const qrStartMatches = (line) => {
  const lower = line.toLowerCase();
  return lower.includes("steam mobile app") && lower.includes("qr code");
};

const isQrArtLine = (line) => {
  if (!line) {
    return false;
  }
  if (/[█▀▄]/.test(line)) {
    return true;
  }
  return /^[\s]+$/.test(line) && line.length >= 10;
};

const updateQrCapture = (job, payload) => {
  if (!job.qrEnabled || payload?.stream !== "stdout") {
    return;
  }
  const line = payload?.line ?? "";

  if (!job.qrCaptureActive && qrStartMatches(line)) {
    job.qrCaptureActive = true;
    job.qrCaptureLines = [];
    openQrModal();
    updateQrModalText(t("qr.waiting"));
    return;
  }

  if (!job.qrCaptureActive) {
    return;
  }

  if (isQrArtLine(line)) {
    job.qrCaptureLines.push(line);
    const trimmed = [...job.qrCaptureLines];
    while (trimmed.length && trimmed[0].trim() === "") {
      trimmed.shift();
    }
    while (trimmed.length && trimmed[trimmed.length - 1].trim() === "") {
      trimmed.pop();
    }
    job.qrText = trimmed.join("\n");
    updateQrModalText(job.qrText);
    return;
  }

  if (job.qrCaptureLines.length > 0) {
    job.qrCaptureActive = false;
  }
};

const qrLoginSuccessMatches = (line) =>
  line.includes("Success! Next time you can login with -username");

const mapStatusToJobState = (payload) => {
  if (!payload?.status) {
    return null;
  }
  switch (payload.status) {
    case "starting":
      return "running";
    case "resolving_metadata":
      return "running"; // Metadata resolution is part of the running phase
    case "running":
      return "running";
    case "finalizing":
      return "running";
    case "compressing":
      return "compressing";
    case "completed":
      return "done";
    case "cancelled":
      return "failed";
    case "finalization_failed":
      return "failed";
    case "error":
      return "failed";
    case "exited":
      return payload?.code === 0 ? "done" : "failed";
    default:
      return null;
  }
};

const extractCompressionPercent = (line) => {
  const match = line.match(/(\d{1,3})%/);
  if (!match) {
    return null;
  }
  const value = Number(match[1]);
  if (!Number.isFinite(value) || value < 0 || value > 100) {
    return null;
  }
  return value;
};

const updateCompressionProgress = (job, line) => {
  const percent = extractCompressionPercent(line);
  if (percent !== null) {
    setCompressionProgress(job, percent);
  }
};

const setCompressionProgress = (job, percent) => {
  if (!Number.isFinite(percent) || percent < 0 || percent > 100) {
    return;
  }
  if (job.compressionProgress !== percent) {
    job.compressionProgress = percent;
    if (job.status === "compressing") {
      renderQueue();
    }
  }
};

const setActiveTab = (name) => {
  activeTab = name;
  tabs.forEach((tab) => {
    tab.classList.toggle("active", tab.dataset.tab === name);
  });
  panels.forEach((panel) => {
    panel.classList.toggle("active", panel.id === `tab-${name}`);
  });
  if (name === "console") {
    consoleRenderState.needsFullRender = true;
    renderConsole(true);
  }
};

tabs.forEach((tab) => {
  tab.addEventListener("click", () => {
    setActiveTab(tab.dataset.tab);
  });
});

// ── Custom checkbox dropdowns (OS + Branch) ───────────────────────────────
// Both the OS panel and the Branch panel are framework-free dropdowns: a button
// shows a summary of the current selection and opens a panel of checkboxes.
// "Add to Queue" then fans out across the cartesian product of the checked OS
// arches and the checked branches (see addJobToQueue).

const getDropdownButton = (dropdown) =>
  dropdown?.querySelector(".checkbox-dropdown-button") || null;

const getDropdownPanel = (dropdown) =>
  dropdown?.querySelector(".checkbox-dropdown-panel") || null;

const getDropdownSummaryEl = (dropdown) =>
  dropdown?.querySelector(".checkbox-dropdown-summary") || null;

const closeDropdown = (dropdown) => {
  const button = getDropdownButton(dropdown);
  const panel = getDropdownPanel(dropdown);
  if (!button || !panel) {
    return;
  }
  panel.hidden = true;
  dropdown.classList.remove("drop-up");
  button.setAttribute("aria-expanded", "false");
};

// Open the panel above the button when there isn't enough room below it (e.g.
// the Branch panel near the bottom of the window would otherwise overflow).
const positionDropdownPanel = (dropdown) => {
  const button = getDropdownButton(dropdown);
  const panel = getDropdownPanel(dropdown);
  if (!button || !panel) {
    return;
  }
  const buttonRect = button.getBoundingClientRect();
  const spaceBelow = window.innerHeight - buttonRect.bottom;
  const spaceAbove = buttonRect.top;
  const needed = panel.offsetHeight + 4;
  const dropUp = spaceBelow < needed && spaceAbove > spaceBelow;
  dropdown.classList.toggle("drop-up", dropUp);
};

const closeAllDropdowns = (except) => {
  [osDropdown, branchDropdown].forEach((dropdown) => {
    if (dropdown && dropdown !== except) {
      closeDropdown(dropdown);
    }
  });
};

const toggleDropdown = (dropdown) => {
  const button = getDropdownButton(dropdown);
  const panel = getDropdownPanel(dropdown);
  if (!button || !panel || button.disabled) {
    return;
  }
  const willOpen = panel.hidden;
  closeAllDropdowns(dropdown);
  panel.hidden = !willOpen;
  button.setAttribute("aria-expanded", willOpen ? "true" : "false");
  if (willOpen) {
    // Panel must be visible before we can measure its height.
    positionDropdownPanel(dropdown);
  }
};

const setDropdownDisabled = (dropdown, disabled) => {
  const button = getDropdownButton(dropdown);
  if (!button) {
    return;
  }
  button.disabled = disabled;
  if (disabled) {
    closeDropdown(dropdown);
  }
};

// ── OS dropdown ───────────────────────────────────────────────────────────
const getOsCheckboxes = () =>
  osDropdown
    ? Array.from(osDropdown.querySelectorAll(".checkbox-dropdown-option input"))
    : [];

const getSelectedOsValues = () =>
  getOsCheckboxes()
    .filter((box) => box.checked)
    .map((box) => box.value);

const updateOsSummary = () => {
  const summary = getDropdownSummaryEl(osDropdown);
  if (!summary) {
    return;
  }
  const selected = getSelectedOsValues();
  if (selected.length === 0) {
    summary.textContent = t("dropdown.none");
  } else {
    summary.textContent = selected.map((os) => formatOsLabel(os)).join(", ");
  }
};

if (osDropdown) {
  const button = getDropdownButton(osDropdown);
  if (button) {
    button.addEventListener("click", () => toggleDropdown(osDropdown));
  }
  getOsCheckboxes().forEach((box) => {
    box.addEventListener("change", () => {
      updateOsSummary();
      syncAddToQueueButton();
    });
  });
  updateOsSummary();
}

// ── Branch dropdown ───────────────────────────────────────────────────────
// `public` is always present and cannot be removed. Typed branches are stored
// in branchState and rendered as rows with an inline optional password field.
const branchState = {
  // entries: { name, checked, password, removable }
  entries: [{ name: "public", checked: true, password: "", removable: false }],
};

const getSelectedBranches = () =>
  branchState.entries
    .filter((entry) => entry.checked)
    .map((entry) => ({ name: entry.name, password: entry.password || "" }));

const updateBranchSummary = () => {
  const summary = getDropdownSummaryEl(branchDropdown);
  if (!summary) {
    return;
  }
  const selected = getSelectedBranches();
  if (selected.length === 0) {
    summary.textContent = t("dropdown.none");
  } else {
    summary.textContent = selected.map((entry) => entry.name).join(", ");
  }
};

const renderBranchOptions = () => {
  if (!branchOptionsContainer) {
    return;
  }
  branchOptionsContainer.textContent = "";
  branchState.entries.forEach((entry, index) => {
    const row = document.createElement("div");
    row.className = "branch-option";

    const main = document.createElement("label");
    main.className = "branch-option-main";

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = entry.checked;
    checkbox.addEventListener("change", () => {
      entry.checked = checkbox.checked;
      updateBranchSummary();
      syncAddToQueueButton();
    });

    const name = document.createElement("span");
    name.className = "branch-option-name";
    name.textContent = entry.name;

    main.appendChild(checkbox);
    main.appendChild(name);
    row.appendChild(main);

    const passwordInput = document.createElement("input");
    passwordInput.type = "password";
    passwordInput.className = "branch-option-password";
    passwordInput.value = entry.password || "";
    passwordInput.placeholder = t("branch.passwordPlaceholder");
    passwordInput.addEventListener("input", () => {
      entry.password = passwordInput.value;
    });
    row.appendChild(passwordInput);

    if (entry.removable) {
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "branch-option-remove";
      remove.textContent = "✕";
      remove.title = t("branch.remove");
      remove.addEventListener("click", () => {
        branchState.entries.splice(index, 1);
        renderBranchOptions();
        updateBranchSummary();
        syncAddToQueueButton();
      });
      row.appendChild(remove);
    }

    branchOptionsContainer.appendChild(row);
  });
};

const addBranchEntry = () => {
  if (!branchAddInput) {
    return;
  }
  const name = branchAddInput.value.trim();
  if (!name) {
    return;
  }
  const exists = branchState.entries.some(
    (entry) => entry.name.toLowerCase() === name.toLowerCase(),
  );
  if (exists) {
    branchAddInput.value = "";
    return;
  }
  branchState.entries.push({
    name,
    checked: true,
    password: "",
    removable: true,
  });
  branchAddInput.value = "";
  renderBranchOptions();
  updateBranchSummary();
  syncAddToQueueButton();
};

if (branchDropdown) {
  const button = getDropdownButton(branchDropdown);
  if (button) {
    button.addEventListener("click", () => toggleDropdown(branchDropdown));
  }
  if (branchAddButton) {
    branchAddButton.addEventListener("click", addBranchEntry);
  }
  if (branchAddInput) {
    branchAddInput.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        addBranchEntry();
      }
    });
  }
  renderBranchOptions();
  updateBranchSummary();
}

// ── Profile dropdown ──────────────────────────────────────────────────────
// Each row separates the two selection actions: the checkbox toggles whether a
// profile is generated at job time (settingsState.selectedProfiles), while
// clicking the name loads it into the editor scratchpad. Built-ins (standard,
// crew) cannot be deleted, so they show no trash icon.
// The collapsed dropdown button reflects the profile currently loaded in the
// editor (updated on name-click load and on save), not the generation checkbox
// selection — that lives in the expanded rows.
const updateProfileSummary = () => {
  const summary = getDropdownSummaryEl(profileDropdown);
  if (!summary) {
    return;
  }
  summary.textContent = templateState.editorProfileName
    ? profileDisplayName(templateState.editorProfileName)
    : t("dropdown.none");
};

const renderProfileOptions = () => {
  if (!profileOptionsContainer) {
    return;
  }
  profileOptionsContainer.textContent = "";

  // Header clarifying that the checkboxes pick which profiles get generated for
  // real job runs (distinct from clicking a name to edit it).
  const header = document.createElement("div");
  header.className = "profile-option-header";
  header.textContent = t("template.profile.generateHint");
  profileOptionsContainer.appendChild(header);

  profileState.profiles.forEach((profile) => {
    const row = document.createElement("div");
    row.className = "profile-option";
    if (
      profile.name.toLowerCase() ===
      String(templateState.editorProfileName || "").toLowerCase()
    ) {
      row.classList.add("is-editing");
    }

    const main = document.createElement("label");
    main.className = "profile-option-main";

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.title = t("template.profile.generateHint");
    checkbox.checked = settingsState.selectedProfiles.includes(profile.name);
    checkbox.addEventListener("change", () => {
      const set = new Set(settingsState.selectedProfiles);
      if (checkbox.checked) {
        set.add(profile.name);
      } else {
        set.delete(profile.name);
      }
      settingsState.selectedProfiles = profileState.profiles
        .map((p) => p.name)
        .filter((name) => set.has(name));
      saveSettings();
      updateProfileSummary();
    });
    main.appendChild(checkbox);

    const name = document.createElement("button");
    name.type = "button";
    name.className = "profile-option-name";
    name.textContent = profileDisplayName(profile.name);
    name.title = t("template.profile.editHint");
    name.addEventListener("click", async () => {
      if (
        profile.name.toLowerCase() ===
        String(templateState.editorProfileName || "").toLowerCase()
      ) {
        return;
      }
      if (!(await confirmDiscardScratchpad())) {
        return;
      }
      loadProfileIntoEditor(profile.name);
      setTemplateStatus(t("template.status.loaded"));
    });
    main.appendChild(name);
    row.appendChild(main);

    if (!profile.builtin) {
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "profile-option-remove";
      remove.textContent = "🗑";
      remove.title = t("template.profile.deleteHint");
      remove.addEventListener("click", (event) => {
        event.stopPropagation();
        void deleteProfile(profile.name);
      });
      row.appendChild(remove);
    }

    profileOptionsContainer.appendChild(row);
  });
};

if (profileDropdown) {
  const button = getDropdownButton(profileDropdown);
  if (button) {
    button.addEventListener("click", () => toggleDropdown(profileDropdown));
  }
  updateProfileSummary();
}

// Close dropdowns when clicking outside of them.
document.addEventListener("click", (event) => {
  if (osDropdown && !osDropdown.contains(event.target)) {
    closeDropdown(osDropdown);
  }
  if (branchDropdown && !branchDropdown.contains(event.target)) {
    closeDropdown(branchDropdown);
  }
  if (profileDropdown && !profileDropdown.contains(event.target)) {
    closeDropdown(profileDropdown);
  }
});

const tauriEvent = window.__TAURI__?.event;
const tauriInvoke = window.__TAURI__?.core?.invoke;

const warnOrphanEvent = (eventName, payload) => {
  console.debug(
    `[OmniPacker] Dropped ${eventName} (no running job).`,
    payload,
  );
};

if (tauriEvent?.listen) {
  tauriEvent.listen("dd:log", (event) => {
    const job = resolveEventJob(event.payload);
    if (!job) {
      warnOrphanEvent("dd:log", event.payload);
      return;
    }
    appendLogToJob(job, event.payload);
    updateQrCapture(job, event.payload);
    if (
      event.payload?.line?.includes(
        "Couldn't find any depots to download for app",
      )
    ) {
      pushJobLog(job, t("dd.noDepots"));
    }
    if (job.qrEnabled && qrLoginSuccessMatches(event.payload?.line ?? "")) {
      rememberQrLogin(job, event.payload?.line ?? "");
      closeQrModal();
    }

    // Steam Guard mobile app confirmation detection
    const logLine = event.payload?.line ?? "";
    if (steamGuardPromptMatches(logLine)) {
      job.steamGuardPending = true;
      openSteamGuardModal();
    }
    if (job.steamGuardPending && steamGuardConfirmedMatches(logLine)) {
      job.steamGuardPending = false;
      closeSteamGuardModal();
    }

    if (steamGuardEmailPromptMatches(logLine)) {
      job.steamGuardEmailPending = true;
      openSteamGuardEmailModal(logLine, job.id);
    }
    if (steamGuardEmailFailureMatches(logLine)) {
      job.steamGuardEmailPending = false;
      closeSteamGuardEmailModal();
    }
    if (steamGuardEmailInvalidCodeMatches(logLine)) {
      requestSteamGuardEmailRetry(job);
    }
    scheduleConsoleRender();
  });

  tauriEvent.listen("dd:status", (event) => {
    const job = resolveEventJob(event.payload);
    if (!job) {
      warnOrphanEvent("dd:status", event.payload);
      return;
    }
    const nextStatus = mapStatusToJobState(event.payload);
    if (nextStatus) {
      const wasRunning = jobState.runningJobId === job.id;
      const retryRequested =
        steamGuardEmailRetryState.requested &&
        steamGuardEmailRetryState.jobId === job.id;
      const isTerminal = nextStatus === "done" || nextStatus === "failed";
      if (retryRequested && wasRunning && isTerminal) {
        resetJobForRetry(job);
        jobState.runningJobId = null;
        steamGuardEmailRetryState.requested = false;
        steamGuardEmailRetryState.jobId = null;
        closeQrModal();
        closeSteamGuardModal();
        closeSteamGuardEmailModal();
        renderAll();
        void startJob();
        return;
      }
      const previousStatus = job.status;
      job.status = nextStatus;
      if (nextStatus === "compressing") {
        if (!Number.isFinite(job.compressionProgress)) {
          job.compressionProgress = 0;
        }
      } else if (previousStatus === "compressing") {
        job.compressionProgress = null;
      }
      // A user cancel halts the entire queue rather than advancing to the next
      // job. The backend emits "cancelled" for both download- and
      // compression-phase cancels; everything else advances as before.
      const wasCancelled = event.payload?.status === "cancelled";
      if (wasRunning && (nextStatus === "done" || nextStatus === "failed")) {
        jobState.runningJobId = null;
        closeQrModal();
        closeSteamGuardModal();
        closeSteamGuardEmailModal();
      }
      renderAll();
      if (
        wasRunning &&
        !wasCancelled &&
        (nextStatus === "done" || nextStatus === "failed")
      ) {
        const nextJob = getNextQueuedJob();
        if (nextJob) {
          void startJob();
        } else {
          authState.rememberedUsername = null;
        }
      }
    }
  });

  tauriEvent.listen("dd:output_conflict", (event) => {
    const payload = event.payload ?? {};
    const job = resolveEventJob(payload);
    if (!job) {
      warnOrphanEvent("dd:output_conflict", payload);
    } else {
      pushJobLog(job, t("output.conflict.log", { path: payload.outputPath || "" }));
      renderAll();
    }
    openOutputConflictModal(payload);
  });

  // Listen for 7-Zip logs during compression
  tauriEvent.listen("7z:log", (event) => {
    const job = getRunningJob();
    if (!job) {
      warnOrphanEvent("7z:log", event.payload);
      return;
    }

    const line = event.payload?.line ?? "";
    const stream = event.payload?.stream ? `[7z:${event.payload.stream}] ` : "[7z] ";
    pushJobLog(job, `${stream}${line}`, { forwardToDebug: false });
    updateCompressionProgress(job, line);
    scheduleConsoleRender();
  });

  tauriEvent.listen("7z:progress", (event) => {
    const job = getRunningJob();
    if (!job) {
      warnOrphanEvent("7z:progress", event.payload);
      return;
    }
    const percent = Number(event.payload?.percent);
    if (!Number.isFinite(percent)) {
      return;
    }
    setCompressionProgress(job, percent);
  });

  // Listen for 7-Zip status (for debugging)
  tauriEvent.listen("7z:status", (event) => {
    const job = getRunningJob();
    if (!job) {
      warnOrphanEvent("7z:status", event.payload);
      return;
    }

    if (event.payload?.status) {
      pushJobLog(job, t("zip.status", { status: event.payload.status }));
      scheduleConsoleRender();
    }
  });
}

// Build job metadata object for backend
const buildJobMetadata = (job) => ({
  appId: job.appId || "unknown",
  os: job.os || "Windows x64",
  branch: job.branch || "public",
  branchPassword: job.branchPassword || "",
  username: job.username || "",
  password: job.password || "",
  qrEnabled: Boolean(job.qrEnabled),
  // When the user has not saved their credentials, tell DepotDownloader to wipe
  // its stored login token after the final queue job. Earlier jobs still reuse
  // the token (no Steam Guard re-prompt mid-queue); nothing persists afterward.
  clearToken: !hasSavedLogin() && isLastQueuedJob(job),
  skipCompression: settingsState.skipCompression,
  compressionPasswordEnabled: settingsState.compressionPasswordEnabled,
  compressionPassword: settingsState.compressionPassword,
  customCompressionArgs: settingsState.customCompressionArgs,
  splitVolumeSize: resolveSplitVolumeSize(),
  // Global uploader handle, fed into the {{username}} token.
  uploaderName: settingsState.uploaderName || "",
  // Resolved profiles selected for generation. Each yields its own .txt.
  templateProfiles: resolveSelectedProfilesForGeneration(),
});

// Resolves the names in settingsState.selectedProfiles into { name, blocks }
// payloads for the backend, expanding built-ins from their presets. Unknown
// names are skipped; an empty result lets the backend fall back to the default.
const resolveSelectedProfilesForGeneration = () =>
  settingsState.selectedProfiles
    .map((name) => {
      const profile = findProfile(name);
      if (!profile) {
        return null;
      }
      let blocks;
      try {
        blocks = parseTemplatePayload(profile.payload);
      } catch (error) {
        return null;
      }
      return {
        name: profile.name,
        blocks: serializeTemplateBlocks(blocks).blocks,
      };
    })
    .filter((entry) => entry !== null);

const startJob = async () => {
  if (jobState.runningJobId) {
    const selectedJob = getSelectedJob();
    const message = t("job.runningMessage");
    if (selectedJob) {
      pushJobLog(selectedJob, message);
      renderAll();
    } else {
      console.debug(`[OmniPacker] ${message}`);
    }
    return;
  }

  const selectedJob = getSelectedJob();
  const jobToRun = getNextQueuedJob();

  if (!jobToRun) {
    if (selectedJob) {
      pushJobLog(selectedJob, t("job.noQueued"));
      renderAll();
    } else {
      console.debug("[OmniPacker] No queued jobs.");
    }
    return;
  }

  syncAuthFromForm(jobToRun);
  applyRememberedAuth(jobToRun);
  jobState.runningJobId = jobToRun.id;
  jobState.selectedJobId = jobToRun.id;
  renderAll();

  if (jobToRun.qrEnabled) {
    openQrModal();
    updateQrModalText(t("qr.waiting"));
  }

  if (!tauriInvoke) {
    jobToRun.status = "failed";
    pushJobLog(jobToRun, t("job.tauriUnavailable"));
    console.debug("[OmniPacker] Tauri invoke unavailable for Start.");
    jobState.runningJobId = null;
    closeQrModal();
    closeSteamGuardModal();
    renderAll();
    return;
  }

  try {
    const jobMetadata = buildJobMetadata(jobToRun);
    try {
      const outputFolder = await tauriInvoke("get_output_folder");
      if (outputFolder) {
        pushJobLog(jobToRun, t("job.outputDir", { path: outputFolder }));
        renderAll();
      }
    } catch (error) {
      pushJobLog(jobToRun, t("job.outputDirError", { error }));
      renderAll();
    }
    pushJobLog(jobToRun, t("job.selectedOs", { os: formatOsLabel(jobToRun.os) }));
    pushJobLog(jobToRun, t("job.starting", { appId: jobToRun.appId }));
    renderAll();
    // Backend returns the job_id (staging directory name)
    const backendJobId = await tauriInvoke("run_depotdownloader", { job: jobMetadata });
    jobToRun.backendJobId = backendJobId;
  } catch (error) {
    jobToRun.status = "failed";
    pushJobLog(jobToRun, t("job.startFailed", { error }));
    jobState.runningJobId = null;
    renderAll();
  }
};

const cancelJob = async () => {
  if (!jobState.runningJobId) {
    console.debug("[OmniPacker] No job is running.");
    return;
  }

  const job = jobState.jobs.get(jobState.runningJobId);
  if (!job) {
    console.warn("[OmniPacker] Running job not found in state.");
    return;
  }

  try {
    pushJobLog(job, t("job.canceling"));
    renderAll();

    const command = job.status === "compressing" ? "cancel_7zip" : "cancel_depotdownloader";
    await tauriInvoke(command);

    pushJobLog(job, t("job.cancelled"));
    renderAll();
  } catch (error) {
    const errorMsg = t("job.cancelFailed", { error });
    pushJobLog(job, errorMsg);
    console.error("[OmniPacker]", errorMsg);
    renderAll();
  }
};

const updateStartButtonState = () => {
  if (!startButton) return;

  if (isQueueRunning()) {
    startButton.textContent = t("action.cancel");
    startButton.classList.add("cancel-mode");
  } else {
    startButton.textContent = t("action.start");
    startButton.classList.remove("cancel-mode");
  }
};

if (startButton) {
  startButton.addEventListener("click", () => {
    if (isQueueRunning()) {
      void cancelJob();
    } else {
      void startJob();
    }
  });
}

const openOutputFolder = async () => {
  if (!tauriInvoke) {
    appendSystemMessage(t("output.unavailable"));
    return;
  }

  try {
    await tauriInvoke("open_output_folder");
  } catch (error) {
    appendSystemMessage(t("output.failed", { error }));
  }
};

if (openOutputButton) {
  openOutputButton.addEventListener("click", () => {
    void openOutputFolder();
  });
}

const saveLoginDetails = async () => {
  const username = steamUsernameInput?.value?.trim() ?? "";
  const password = steamPasswordInput?.value ?? "";

  if (!username || !password) {
    alert(t("auth.saveMissing"));
    return;
  }

  if (!tauriInvoke) {
    alert(t("auth.saveFailed", { error: t("auth.tauriUnavailable") }));
    return;
  }

  try {
    await tauriInvoke("save_login_data", { username, password });
    setSavedLogin({ username, password });
    renderAll();
  } catch (error) {
    alert(t("auth.saveFailed", { error: String(error) }));
  }
};

const loadSavedLoginDetails = async () => {
  if (!tauriInvoke) {
    return;
  }

  try {
    const saved = await tauriInvoke("load_login_data");
    if (saved?.username && saved?.password) {
      setSavedLogin(saved);
      renderAll();
    }
  } catch (error) {
    console.debug("[OmniPacker] Failed to load login data:", error);
  }
};

const deleteSavedLoginDetails = async () => {
  if (!tauriInvoke) {
    alert(t("auth.deleteFailed", { error: t("auth.tauriUnavailable") }));
    return;
  }

  try {
    await tauriInvoke("delete_login_data");
    setSavedLogin(null);
    renderAll();
  } catch (error) {
    alert(t("auth.deleteFailed", { error: String(error) }));
  }
};

// One "Add to Queue" click fans out into the cartesian product of the checked
// OS arches and the checked branches, all sharing the same credentials. With a
// single OS and a single branch this behaves exactly like the old single-job
// flow.
const addJobToQueue = () => {
  const credentials = getCredentialSnapshot();
  const osValues = getSelectedOsValues();
  const branches = getSelectedBranches();
  if (osValues.length === 0 || branches.length === 0) {
    return;
  }

  let lastJob = null;
  osValues.forEach((os) => {
    branches.forEach((branch) => {
      lastJob = createJob({
        ...credentials,
        os,
        branch: branch.name,
        branchPassword: branch.password,
      });
      lastJob.status = "queued";
    });
  });

  if (lastJob) {
    jobState.selectedJobId = lastJob.id;
  }
  if (appIdInput) {
    appIdInput.value = "";
  }
  renderAll();
};

if (addToQueueButton) {
  addToQueueButton.addEventListener("click", addJobToQueue);
}

if (clearQueueButton) {
  clearQueueButton.addEventListener("click", () => {
    clearQueuedJobs();
    renderAll();
  });
}

if (appIdInput) {
  appIdInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      addJobToQueue();
    }
  });
}

const copyQrText = async () => {
  const selectedJob = getSelectedJob();
  const text = selectedJob?.qrText;
  if (!text) {
    return;
  }
  if (navigator?.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const textarea = document.createElement("textarea");
  textarea.value = text;
  document.body.appendChild(textarea);
  textarea.select();
  document.execCommand("copy");
  document.body.removeChild(textarea);
};

const renderQueue = () => {
  if (!queueList) {
    return;
  }

  queueList.innerHTML = "";

  if (jobState.order.length === 0) {
    const empty = document.createElement("div");
    empty.className = "queue-empty";
    empty.textContent = t("queue.empty");
    queueList.appendChild(empty);
    if (clearQueueButton) {
      clearQueueButton.disabled = true;
    }
    return;
  }

  if (clearQueueButton) {
    clearQueueButton.disabled = isQueueRunning();
  }

  for (let index = 0; index < jobState.order.length; index += 1) {
    const jobId = jobState.order[index];
    const job = jobState.jobs.get(jobId);
    if (!job) {
      continue;
    }
    const row = document.createElement("div");
    row.setAttribute("role", "option");
    row.tabIndex = 0;
    row.className = "queue-item";
    row.dataset.jobId = job.id;

    if (job.id === jobState.selectedJobId) {
      row.classList.add("selected");
    }

    if (job.status === "failed") {
      row.classList.add("failed");
    }

    const header = document.createElement("div");
    header.className = "queue-item-header";
    header.textContent = t("queue.appId", { appId: job.appId });

    const top = document.createElement("div");
    top.className = "queue-item-top";

    const status = document.createElement("div");
    status.className = `queue-item-status status-${job.status}`;
    let statusText = formatStatus(job.status);
    if (job.status === "compressing") {
      const progress = Number.isFinite(job.compressionProgress)
        ? job.compressionProgress
        : 0;
      statusText = `${statusText} ${progress}%`;
    }
    status.textContent = statusText;

    const controls = document.createElement("div");
    controls.className = "queue-item-controls";

    const isRunning = isRunningJob(job.id);
    const queueRunning = isQueueRunning();
    const disableReason = queueRunning
      ? t("queue.running")
      : t("queue.noReorder");

    const upButton = document.createElement("button");
    upButton.type = "button";
    upButton.className = "queue-btn queue-btn-up";
    upButton.textContent = "▲";
    upButton.disabled = queueRunning || isRunning || index === 0;
    if (upButton.disabled) {
      upButton.title = queueRunning
        ? disableReason
        : isRunning
          ? disableReason
          : t("queue.atTop");
    }
    upButton.addEventListener("click", (event) => {
      event.stopPropagation();
      moveJob(job.id, -1);
      renderAll();
    });

    const downButton = document.createElement("button");
    downButton.type = "button";
    downButton.className = "queue-btn queue-btn-down";
    downButton.textContent = "▼";
    downButton.disabled =
      queueRunning || isRunning || index === jobState.order.length - 1;
    if (downButton.disabled) {
      downButton.title = queueRunning
        ? disableReason
        : isRunning
          ? disableReason
          : t("queue.atBottom");
    }
    downButton.addEventListener("click", (event) => {
      event.stopPropagation();
      moveJob(job.id, 1);
      renderAll();
    });

    const removeButton = document.createElement("button");
    removeButton.type = "button";
    removeButton.className = "queue-btn queue-btn-trash";
    removeButton.textContent = "🗑";
    removeButton.disabled = queueRunning || isRunning;
    if (removeButton.disabled) {
      removeButton.title = disableReason;
    }
    removeButton.addEventListener("click", (event) => {
      event.stopPropagation();
      removeJob(job.id);
      renderAll();
    });

    controls.appendChild(upButton);
    controls.appendChild(downButton);
    controls.appendChild(removeButton);

    top.appendChild(status);
    top.appendChild(controls);

    const meta = document.createElement("div");
    meta.className = "queue-item-meta";
    meta.textContent = t("queue.meta", {
      branch: job.branch || "public",
      os: formatOsLabel(job.os),
    });

    row.appendChild(header);
    row.appendChild(top);
    row.appendChild(meta);

    row.addEventListener("click", () => {
      jobState.selectedJobId = job.id;
      renderAll();
    });
    row.addEventListener("keydown", (event) => {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        jobState.selectedJobId = job.id;
        renderAll();
      }
    });

    queueList.appendChild(row);
  }
};

const renderConsole = (force = false) => {
  if (!consoleOutput) {
    return;
  }
  if (!force && activeTab !== "console") {
    return;
  }

  if (consoleRenderState.timer) {
    window.clearTimeout(consoleRenderState.timer);
    consoleRenderState.timer = null;
  }

  const selectedJob = getSelectedJob();
  if (!selectedJob) {
    consoleOutput.value = t("console.noSelection");
    consoleRenderState.jobId = null;
    consoleRenderState.renderedLines = 0;
    consoleRenderState.needsFullRender = true;
    return;
  }

  if (consoleRenderState.jobId !== selectedJob.id) {
    consoleRenderState.jobId = selectedJob.id;
    consoleRenderState.renderedLines = 0;
    consoleRenderState.needsFullRender = true;
  }

  if (consoleRenderState.renderedLines > selectedJob.logs.length) {
    consoleRenderState.needsFullRender = true;
  }

  if (selectedJob.logs.length === 0) {
    consoleOutput.value = "";
    consoleRenderState.renderedLines = 0;
    consoleRenderState.needsFullRender = false;
    return;
  }

  if (consoleRenderState.needsFullRender) {
    consoleOutput.value = selectedJob.logs.join("\n");
    consoleRenderState.renderedLines = selectedJob.logs.length;
    consoleRenderState.needsFullRender = false;
  } else if (consoleRenderState.renderedLines < selectedJob.logs.length) {
    const nextLines = selectedJob.logs.slice(consoleRenderState.renderedLines);
    const addition = nextLines.join("\n");
    consoleOutput.value = consoleOutput.value
      ? `${consoleOutput.value}\n${addition}`
      : addition;
    consoleRenderState.renderedLines = selectedJob.logs.length;
  }

  // Auto-scroll to bottom so user sees latest output
  consoleOutput.scrollTop = consoleOutput.scrollHeight;
};

const renderAll = () => {
  renderQueue();
  renderConsole();
  updateFormInputState();
  updateStartButtonState();
};

if (qrLoginToggle) {
  const syncQrLogin = () => {
    const enabled = qrLoginToggle.checked;
    const credentialsPanel = qrLoginToggle.closest(".credentials");
    credentialsPanel?.classList.toggle("qr-enabled", enabled);
    renderAll();
  };
  qrLoginToggle.addEventListener("change", syncQrLogin);
  syncQrLogin();
}

if (qrCloseButton) {
  qrCloseButton.addEventListener("click", closeQrModal);
}

if (qrCopyButton) {
  qrCopyButton.addEventListener("click", () => {
    void copyQrText();
  });
}

if (steamGuardEmailSubmitButton) {
  steamGuardEmailSubmitButton.addEventListener("click", () => {
    void submitSteamGuardEmailCode();
  });
}

if (steamGuardEmailCloseButton) {
  steamGuardEmailCloseButton.addEventListener("click", closeSteamGuardEmailModal);
}

if (steamGuardEmailInput) {
  steamGuardEmailInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void submitSteamGuardEmailCode();
    }
  });
}

if (saveLoginButton) {
  saveLoginButton.addEventListener("click", () => {
    void saveLoginDetails();
  });
}

// Settings modal event listeners
if (settingsButton) {
  settingsButton.addEventListener("click", openSettingsModal);
}

if (settingsCloseButton) {
  settingsCloseButton.addEventListener("click", closeSettingsModal);
}

if (templateEditorButton) {
  templateEditorButton.addEventListener("click", () => {
    void openTemplateEditor();
  });
}

if (templateCloseButton) {
  templateCloseButton.addEventListener("click", closeTemplateEditor);
}

if (templateAddBlockButton) {
  templateAddBlockButton.addEventListener("click", () => {
    const selected = templateBlockSelect?.value || "title";
    addTemplateBlock(selected);
  });
}

if (templateLoadButton) {
  templateLoadButton.addEventListener("click", () => {
    templateLoadInput?.click();
  });
}

if (templateLoadInput) {
  templateLoadInput.addEventListener("change", () => {
    const file = templateLoadInput.files?.[0];
    if (file) {
      importProfileFromFile(file);
    }
    templateLoadInput.value = "";
  });
}

if (templateSaveButton) {
  templateSaveButton.addEventListener("click", () => {
    openSaveProfileModal();
  });
}

if (templateOpenFolderButton) {
  templateOpenFolderButton.addEventListener("click", () => {
    void openProfilesFolder();
  });
}

if (templateSaveConfirmButton) {
  templateSaveConfirmButton.addEventListener("click", () => {
    void confirmSaveProfile();
  });
}

if (templateSaveCancelButton) {
  templateSaveCancelButton.addEventListener("click", () => {
    closeSaveProfileModal();
  });
}

if (templateSaveNameInput) {
  templateSaveNameInput.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void confirmSaveProfile();
    } else if (event.key === "Escape") {
      event.preventDefault();
      closeSaveProfileModal();
    }
  });
}

if (templateSaveOverlay) {
  templateSaveOverlay.addEventListener("click", (event) => {
    if (event.target === templateSaveOverlay) {
      closeSaveProfileModal();
    }
  });
}

if (uploaderNameInput) {
  uploaderNameInput.addEventListener("input", () => {
    settingsState.uploaderName = uploaderNameInput.value;
    saveSettings();
    renderTemplatePreview();
  });
}

if (templateGenericConfirmYes) {
  templateGenericConfirmYes.addEventListener("click", () => {
    closeGenericConfirm(true);
  });
}

if (templateGenericConfirmNo) {
  templateGenericConfirmNo.addEventListener("click", () => {
    closeGenericConfirm(false);
  });
}

if (templateGenericConfirmOverlay) {
  templateGenericConfirmOverlay.addEventListener("click", (event) => {
    if (event.target === templateGenericConfirmOverlay) {
      closeGenericConfirm(false);
    }
  });
}

if (outputConflictOverwriteButton) {
  outputConflictOverwriteButton.addEventListener("click", () => {
    void sendOutputConflictChoice("overwrite");
  });
}

if (outputConflictCopyButton) {
  outputConflictCopyButton.addEventListener("click", () => {
    void sendOutputConflictChoice("copy");
  });
}

if (outputConflictCancelButton) {
  outputConflictCancelButton.addEventListener("click", () => {
    void sendOutputConflictChoice("cancel");
  });
}

if (outputConflictOverlay) {
  outputConflictOverlay.addEventListener("click", (event) => {
    if (event.target === outputConflictOverlay && !outputConflictState.busy) {
      void sendOutputConflictChoice("cancel");
    }
  });
}

if (templateCopyButton) {
  templateCopyButton.addEventListener("click", () => {
    void copyTemplatePreview();
  });
}

if (skipCompressionToggle) {
  skipCompressionToggle.addEventListener("change", () => {
    settingsState.skipCompression = skipCompressionToggle.checked;
    saveSettings();
  });
}

if (compressionPasswordToggle) {
  compressionPasswordToggle.addEventListener("change", () => {
    settingsState.compressionPasswordEnabled = compressionPasswordToggle.checked;
    syncCompressionPasswordUI(true);
    saveSettings();
  });
}

if (compressionPasswordInput) {
  compressionPasswordInput.addEventListener("input", () => {
    settingsState.compressionPassword = compressionPasswordInput.value;
    saveSettings();
  });
}

if (customCompressionArgsInput) {
  customCompressionArgsInput.addEventListener("input", () => {
    settingsState.customCompressionArgs = customCompressionArgsInput.value;
    saveSettings();
  });
}

if (splitArchiveToggle) {
  splitArchiveToggle.addEventListener("change", () => {
    settingsState.splitArchiveEnabled = splitArchiveToggle.checked;
    syncSplitArchiveUI();
    saveSettings();
  });
}

if (splitSizePreset) {
  splitSizePreset.addEventListener("change", () => {
    settingsState.splitSizePreset = splitSizePreset.value;
    syncSplitArchiveUI();
    saveSettings();
  });
}

if (splitCustomSize) {
  splitCustomSize.addEventListener("input", () => {
    settingsState.splitCustomSize = splitCustomSize.value;
    saveSettings();
  });
}

if (splitCustomUnit) {
  splitCustomUnit.addEventListener("change", () => {
    settingsState.splitCustomUnit = splitCustomUnit.value;
    saveSettings();
  });
}

if (defaultQrToggle) {
  defaultQrToggle.addEventListener("change", () => {
    settingsState.defaultQrLogin = defaultQrToggle.checked;
    saveSettings();
    applyDefaultQrLogin();
  });
}

if (languageSelect) {
  languageSelect.addEventListener("change", () => {
    settingsState.language = languageSelect.value;
    saveSettings();
    applyTranslations();
    renderAll();
    console.debug("[OmniPacker] Language changed to:", settingsState.language);
  });
}

if (deleteLoginButton) {
  deleteLoginButton.addEventListener("click", () => {
    void deleteSavedLoginDetails();
  });
}

// --- Update check (check-and-warn, not a self-updater) ---

// Latest UpdateInfo from the backend, kept so the banner buttons can act on it.
let latestUpdateInfo = null;

const showUpdateBanner = (info) => {
  if (!updateBanner || !updateBannerText) {
    return;
  }
  latestUpdateInfo = info;
  updateBannerText.textContent = t("update.available", { version: info.latest });
  updateBanner.hidden = false;
};

const hideUpdateBanner = () => {
  if (updateBanner) {
    updateBanner.hidden = true;
  }
};

// Runs the backend check. `silent` (startup) swallows everything and only shows
// the banner when an update is available and not skipped. Non-silent (manual
// settings button) also reports "up to date" / failure in the settings hint.
const runUpdateCheck = async (silent) => {
  if (!tauriInvoke) {
    return;
  }
  if (!silent && updateStatusLabel) {
    updateStatusLabel.textContent = t("update.checking");
  }
  try {
    const info = await tauriInvoke("check_for_update");
    if (!info?.update_available) {
      if (!silent && updateStatusLabel) {
        updateStatusLabel.textContent = t("update.upToDate");
      }
      return;
    }
    // On startup, respect a previously skipped version.
    if (silent && settingsState.skippedUpdateVersion === info.latest) {
      return;
    }
    if (!silent && updateStatusLabel) {
      updateStatusLabel.textContent = "";
    }
    showUpdateBanner(info);
  } catch (error) {
    // A failed check must never block or error-popup. Surface only on manual use.
    console.debug("[OmniPacker] Update check failed:", error);
    if (!silent && updateStatusLabel) {
      updateStatusLabel.textContent = t("update.checkFailed");
    }
  }
};

if (updateBannerDownload) {
  updateBannerDownload.addEventListener("click", () => {
    if (latestUpdateInfo?.url && tauriInvoke) {
      void tauriInvoke("open_external_url", { url: latestUpdateInfo.url }).catch(
        (error) => console.debug("[OmniPacker] Failed to open release page:", error),
      );
    }
  });
}

if (updateBannerSkip) {
  updateBannerSkip.addEventListener("click", () => {
    if (latestUpdateInfo?.latest) {
      settingsState.skippedUpdateVersion = latestUpdateInfo.latest;
      saveSettings();
    }
    hideUpdateBanner();
  });
}

if (updateBannerDismiss) {
  updateBannerDismiss.addEventListener("click", () => {
    // Session-only dismiss: no persistence, reappears next launch.
    hideUpdateBanner();
  });
}

if (checkUpdateButton) {
  checkUpdateButton.addEventListener("click", () => {
    void runUpdateCheck(false);
  });
}

// Load settings from localStorage on startup
loadSettings();
applySettingsToUI();
applyDefaultQrLogin();
// Seed the profile library so jobs started without opening the editor can still
// resolve their selected profiles (built-ins + saved custom profiles).
void refreshProfileLibrary();
void loadSavedLoginDetails();

// Scale the fixed-size UI to fit the window, preserving proportions.
// Below the design size: use CSS transform to shrink proportionally.
// At or above design size: let the layout flow naturally to fill the space.
const DESIGN_WIDTH = 1140;
const DESIGN_HEIGHT = 760;

function applyUiScale() {
  const scale = Math.min(
    window.innerWidth / DESIGN_WIDTH,
    window.innerHeight / DESIGN_HEIGHT,
  );

  const body = document.body;
  if (scale < 1) {
    // Small window: fixed size + transform scale down
    body.classList.add("fixed-layout");
    body.classList.remove("fluid-layout");
    document.documentElement.style.setProperty("--ui-scale", String(scale));
  } else {
    // Large window: flow naturally
    body.classList.remove("fixed-layout");
    body.classList.add("fluid-layout");
    document.documentElement.style.setProperty("--ui-scale", "1");
  }
}

window.addEventListener("resize", applyUiScale);
applyUiScale();

renderAll();

// Non-blocking update check on launch. Silently no-ops on network failure.
void runUpdateCheck(true);
