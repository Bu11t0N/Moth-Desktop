import type { ConnectionSettings } from "./api";
let language: "ru" | "en" = "ru";
export const currentLanguage = () => language;
const en: Record<string, string> = {
  "Диагностика": "Diagnostics",
  "ДИАГНОСТИКА": "DIAGNOSTICS",
  "О приложении": "About",
  "О ПРИЛОЖЕНИИ": "ABOUT",
  "Соединение, защита и поведение приложения": "Connection, protection and application behavior",
  "Статистика": "Traffic",
  "СТАТИСТИКА": "TRAFFIC",
  "Для VPN нужна служба Moth с протоколом IPC 2 (0.3.1–0.3.5).": "VPN requires the Moth IPC 2 service (0.3.1–0.3.5); new features require 0.3.5.",
  "Автовосстановление включается отдельно в настройках.": "Automatic recovery is enabled separately in settings.",
  "Туннель проверен · kill switch включён": "Tunnel verified · kill switch is on",
  "Туннель проверен · kill switch не подтверждён": "Tunnel verified · kill switch is unverified",
  "Выйти, сохранить блокировку": "Exit and keep network blocked",
  "Kill switch включается отдельно; при включении Disconnect и Exit сохраняют блокировку.": "Kill switch is enabled separately; when enabled, Disconnect and Exit retain the network block.",
  "Резервные копии и перенос Android": "Backups and Android migration",
  "Копия содержит ссылки и пароли серверов. Экспорт защищён отдельным паролем. HWID этого компьютера не переносится. Восстановление не меняет активное подключение; подключитесь заново для применения настроек.":
    "The backup contains server URLs and passwords. Export uses a separate password. This computer's HWID is not transferred. Restore does not change the active connection; reconnect to apply settings.",
  "Пароль резервной копии": "Backup password",
  "Скачать зашифрованную копию": "Download encrypted backup",
  "Копия Desktop или Android": "Desktop or Android backup",
  "Предварительный просмотр": "Preview",
  "Отменить восстановление": "Undo restore",
  "Отмена доступна 10 минут в этом сеансе, пока библиотека не изменилась.":
    "Undo is available for 10 minutes in this session, until the library changes.",
  "Подтверждение восстановления": "Confirm restore",
  "Заменить текущую библиотеку и настройки":
    "Replace current library and settings",
  "Без замены серверы и избранное объединяются; текущие настройки сохраняются. Передача HWID, расписание и автоподключение для перенесённых данных выключены.":
    "Without replacement, servers and favorites are merged and current settings are retained. HWID sharing, schedules and automatic connection are disabled for imported data.",
  Восстановить: "Restore",
  "Обработка резервной копии…": "Processing backup…",
  "Резервные копии доступны в настольном приложении.":
    "Backups are available in the desktop application.",
  "Выберите резервную копию.": "Choose a backup file.",
  "Резервная копия слишком большая": "The backup is too large",
  "Для резервной копии нужен пароль от 8 символов":
    "The backup requires a password of at least 8 characters",
  "Неверный пароль или повреждённая резервная копия":
    "Incorrect password or damaged backup",
  "Для этой резервной копии нужен пароль": "This backup requires a password",
  "Предварительный просмотр истёк. Откройте копию заново":
    "The preview expired. Open the backup again",
  "Библиотека изменилась. Повторите предварительный просмотр":
    "The library changed. Preview the backup again",
  "Откройте резервную копию заново": "Open the backup again",
  "Нет доступной отмены восстановления": "Restore undo is unavailable",
  "HWID не переносится; передача данных устройства, расписание и автоподключение выключены до явного выбора":
    "HWID is not transferred; device data sharing, schedules and automatic connection remain disabled until explicitly enabled",
  "Android package IDs и Android-специфичные настройки не переносятся; проверьте сетевые настройки перед подключением":
    "Android package IDs and Android-specific settings are not transferred; review network settings before connecting",
  "Серверы:": "Servers:",
  "Подписки:": "Subscriptions:",
  "Избранное:": "Favorites:",
  "Правила:": "Rules:",
  мотылёк: "Moth",
  "Мотылёк, главная": "Moth, home",
  Главная: "Home",
  Серверы: "Servers",
  Настройки: "Settings",
  Разделы: "Navigation",
  ОБЗОР: "OVERVIEW",
  СЕРВЕРЫ: "SERVERS",
  НАСТРОЙКИ: "SETTINGS",
  "Версия для разработки": "Development build",
  Версия: "Version",
  Платформа: "Platform",
  Приложение: "Application",
  ПАРАМЕТРЫ: "SETTINGS",
  ПРОФИЛИ: "PROFILES",
  "Подключение VPN": "VPN connection",
  "Выберите сервер и подключитесь к VPN.":
    "Select a server and connect to the VPN.",
  "Добавить сервер": "Add server",
  "Добавить первый сервер": "Add first server",
  "Нет серверов": "No servers",
  "Нет выбранного сервера": "No server selected",
  "Выбранный сервер": "Selected server",
  "Все серверы": "All servers",
  "Открыть список серверов": "Open server list",
  "Добавь ссылку от своего провайдера": "Add a link from your provider",
  "или владельца сервера.": "or server operator.",
  "СИСТЕМНЫЙ ТУННЕЛЬ": "SYSTEM TUNNEL",
  "VPN отключён": "VPN disconnected",
  "VPN подключён": "VPN connected",
  "Подключение VPN…": "Connecting VPN…",
  "Отключение VPN…": "Disconnecting VPN…",
  "Ошибка VPN": "VPN error",
  "Не подключено": "Disconnected",
  "Подключаем…": "Connecting…",
  Подключено: "Connected",
  "Отключаем…": "Disconnecting…",
  "Соединение прервано": "Connection interrupted",
  "Служба недоступна": "Service unavailable",
  "Проверяем службу VPN…": "Checking VPN service…",
  Подключить: "Connect",
  Отключить: "Disconnect",
  "Отменить подключение": "Cancel connection",
  "Выполняем…": "Working…",
  "VPN сейчас не защищает трафик": "VPN is not protecting traffic",
  "Туннель проверен · kill switch пока недоступен":
    "Tunnel verified · kill switch unavailable",
  "Выбери сервер и подключись.": "Select a server and connect.",
  "Создаём туннель и проверяем HTTPS через него…":
    "Creating the tunnel and verifying HTTPS through it…",
  "Служба подтверждает работу обоих сетевых ядер.":
    "The service confirms that both network engines are running.",
  "VPN включён другой учётной записью.": "VPN is owned by another account.",
  "Отключаем туннель…": "Disconnecting the tunnel…",
  "Туннель отключён. Проверь сервер и подключись снова.":
    "The tunnel is disconnected. Check the server and reconnect.",
  "Восстановление подключения…": "Recovering connection…",
  "Восстановление подключения": "Connection recovery",
  "HTTPS-доступ не подтверждён": "HTTPS access not verified",
  "Ошибка проверки VPN": "VPN verification failed",
  "Служба восстанавливает подключение. Можно остановить восстановление кнопкой отключения.":
    "The service is recovering the connection. Use Disconnect to stop recovery.",
  "Туннель работает, но проверка HTTPS-доступа не прошла. Доступность VPN не подтверждена.":
    "The tunnel is running, but the HTTPS check failed. VPN availability is not verified.",
  "Установка и обновление службы": "Service installation and updates",
  "Запустите установщик Moth. Для portable-пакета доступен Install-Service.cmd.":
    "Run the Moth installer. The portable package includes Install-Service.cmd.",
  "Установите пакет Moth .deb. Перед обновлением отключите VPN и выйдите из приложения.":
    "Install the Moth .deb package. Disconnect VPN and exit the app before updating.",
  "Крестик сворачивает Moth в трей.":
    "Closing the window hides Moth in the tray.",
  "Крестик сворачивает окно в трей, а при отсутствии трея — на панель задач.":
    "Closing the window hides it in the tray, or minimizes to the taskbar when no tray is available.",
  "Для остановки VPN и завершения приложения выберите «Выйти» внизу окна или «Выход» в трее.":
    "To stop VPN and quit, select Exit in the window or tray.",
  "Хранение профилей": "Profile storage",
  "Профили защищены хранилищем учётной записи. Без аналитики и облачной синхронизации.":
    "Profiles use your account's protected storage. No analytics or cloud synchronization.",
  Выйти: "Exit",
  Выход: "Exit",
  ИМПОРТ: "IMPORT",
  "Добавить серверы": "Add servers",
  "Ссылки серверов": "Server links",
  "По одной ссылке на строку или Base64-список. Загрузка HTTPS-подписок будет добавлена позже.":
    "One server link per line, a Base64 list, Clash YAML or supported JSON. Use Add subscription for a provider URL.",
  Импортировать: "Import",
  Отмена: "Cancel",
  "Удалить сервер?": "Delete server?",
  "Он исчезнет из локального списка.":
    "It will be removed from the local list.",
  Удалить: "Delete",
  "Проверить сервер": "Check server",
  "Проверяем…": "Checking…",
  Выбрать: "Select",
  Выбран: "Selected",
  "Без TLS": "No TLS",
  "Просмотр интерфейса в браузере. Профили и подключение доступны только в настольном приложении.":
    "Browser preview. Profiles and connections are available only in the desktop app.",
  "Ссылки серверов и подписки провайдеров":
    "Server links and provider subscriptions",
  "Добавить подписку": "Add subscription",
  "HWID устройства": "Device HWID",
  Подписки: "Subscriptions",
  Подписка: "Subscription",
  Название: "Name",
  "Ссылка подписки": "Subscription URL",
  "Ссылка хранится в защищённом хранилище. При редактировании оставьте поле пустым, чтобы сохранить прежнюю.":
    "The URL stays in protected storage. Leave it blank when editing to keep the saved URL.",
  "Разрешить HTTP: секретная ссылка и ответ передаются без шифрования":
    "Allow HTTP: the secret URL and response are transmitted without encryption",
  "Отправлять HWID этому провайдеру": "Send HWID to this provider",
  "Также отправлять сведения об ОС и типе устройства":
    "Also send OS and device type information",
  "Данные устройства отправляются только по HTTPS в пределах исходного origin. Лимиты и регистрацию устройства определяет провайдер.":
    "Device data is sent only over HTTPS to the original origin. Device registration and limits are controlled by the provider.",
  Обновление: "Refresh",
  Вручную: "Manually",
  "Каждые 6 часов": "Every 6 hours",
  "Каждые 12 часов": "Every 12 hours",
  "Раз в сутки": "Daily",
  "Обновлять только через мой активный VPN":
    "Refresh only through my active VPN",
  Сохранить: "Save",
  Изменить: "Edit",
  Обновить: "Refresh",
  "Обновляем…": "Refreshing…",
  "Серверов:": "Servers:",
  "Обновлено:": "Last success:",
  Каждые: "Every",
  "ч при работающем приложении": "hours while the app is running",
  "Только через VPN": "VPN only",
  "Трафик:": "Traffic:",
  "Срок:": "Expires:",
  ГБ: "GB",
  "Локальные серверы": "Local servers",
  Избранное: "Favorites",
  "Сохранённые из прежнего списка": "Retained from an earlier list",
  Поиск: "Search",
  "Название, адрес или протокол": "Name, address or protocol",
  Сортировка: "Sort",
  "По подписке": "Source order",
  "По названию": "Name",
  "По задержке": "Latency",
  "Проверить видимые": "Check visible servers",
  "Отменить проверку": "Cancel queued checks",
  "Ничего не найдено.": "No matches.",
  "Добавьте ссылку сервера или подписку провайдера.":
    "Add a server link or provider subscription.",
  "HWID этого устройства": "This device's HWID",
  "Стабильный случайный идентификатор этой установки Moth. Зарегистрируйте его у провайдера, если требуется привязка устройства. Идентификатор телефона не используется.":
    "A stable random identifier for this Moth installation. Register it with your provider if device binding is required. Your phone's identifier is not used.",
  Закрыть: "Close",
  Скопировать: "Copy",
  Скопировано: "Copied",
  "Выделите HWID и скопируйте вручную": "Select the HWID and copy it manually",
  "Удалить подписку и её серверы?": "Delete the subscription and its servers?",
  "У провайдера подписка продолжит действовать.":
    "The provider subscription will remain active.",
  "В избранное": "Add to favorites",
  "Убрать из избранного": "Remove from favorites",
  "Сеть и поведение подключения": "Network and connection behavior",
  "Сетевые настройки применяются при следующем подключении. Общий DNS остаётся внутри VPN, в том числе при разделении приложений.":
    "Network settings apply on the next connection. Shared DNS stays inside VPN, including when application splitting is enabled.",
  "DNS (IP или HTTPS URL)": "DNS (IP or HTTPS URL)",
  "Bootstrap DNS (числовой IP в адресе)":
    "Bootstrap DNS (numeric IP in the address)",
  "IPv6 через VPN; при выключении IPv6 в туннеле блокируется":
    "IPv6 through VPN; disabling it blocks IPv6 in the tunnel",
  "Правила маршрутизации": "Routing rules",
  "Правила применяются сверху вниз. VPN-only обновления подписок всегда идут через VPN.":
    "Rules apply from top to bottom. VPN-only subscription updates always use VPN.",
  Правило: "Rule",
  Включено: "Enabled",
  "Название правила": "Rule name",
  "Тип правила": "Rule type",
  Домен: "Domain",
  "Суффикс домена": "Domain suffix",
  "Ключевое слово": "Keyword",
  "Значение правила": "Rule value",
  "Действие правила": "Rule action",
  Напрямую: "Direct",
  Блокировать: "Block",
  "Переместить правило выше": "Move rule up",
  "Удалить правило": "Delete rule",
  "Добавить правило": "Add rule",
  "Разделение приложений": "Application splitting",
  Режим: "Mode",
  "Выбранные приложения напрямую": "Selected applications bypass VPN",
  "Только выбранные приложения через VPN": "Only selected applications use VPN",
  "Абсолютные пути исполняемых файлов, по одному на строку":
    "Absolute executable paths, one per line",
  "Используются пути процессов этой ОС, а не Android package IDs. Пустой список отправляет все приложения через VPN. Процессы, которые ОС не смогла определить, следуют правилу по умолчанию выбранного режима.":
    "Use executable paths for this OS, not Android package IDs. An empty list puts all applications in VPN. Unidentified processes follow the selected mode's default rule.",
  "Восстановление и проверки": "Recovery and health checks",
  "Восстанавливать подключение после сбоя, сна или смены сети":
    "Recover after failure, sleep or a network change",
  "Проверять доступность HTTPS через активный туннель":
    "Check HTTPS availability through the active tunnel",
  "После повторных ошибок выбирать доступный сервер":
    "Choose an available server after repeated failures",
  "Подключаться к последнему серверу при запуске приложения":
    "Connect to the last server when the app starts",
  "URL HTTPS-проверки": "HTTPS health-check URL",
  Интервал: "Interval",
  "1 минута": "1 minute",
  "2 минуты": "2 minutes",
  "5 минут": "5 minutes",
  "10 минут": "10 minutes",
  "15 минут": "15 minutes",
  Интерфейс: "Interface",
  Тема: "Theme",
  Системная: "System",
  Светлая: "Light",
  Тёмная: "Dark",
  Язык: "Language",
  "Отменить изменения": "Discard changes",
  "Сохранить настройки": "Save settings",
  "Состояние этой сборки и доступные возможности":
    "Build status and available capabilities",
  "Защищённое хранилище": "Protected storage",
  "Недоступно в этом режиме": "Unavailable in this mode",
  "Системный VPN": "System VPN",
  "Доступен через службу": "Available through the service",
  "Не подтверждён": "Not verified",
  "Не реализован": "Not implemented",
  Доступен: "Available",
  "Служба VPN": "VPN service",
  "Ещё не проверена": "Not checked yet",
  "Состояние службы": "Service status",
  "Протокол управления": "Control protocol",
  "Совместим · IPC 2": "Compatible · IPC 2",
  Несовместим: "Incompatible",
  "Ядро службы:": "Service engine:",
  Проверено: "Verified",
  Недоступно: "Unavailable",
  "Проверить связь со службой": "Check service connection",
  "Ядра приложения": "Application engines",
  "Хэш проверен при запуске": "Hash checked at startup",
  "HTTPS-проверка проходит через выбранный сервер. Её результат — не ICMP ping и не признак включённого системного VPN.":
    "HTTPS checks use the selected server. Their result is neither ICMP ping nor confirmation of system VPN protection.",
  Ограничения: "Limitations",
  "Без автовосстановления после смены сети подключитесь заново.":
    "Reconnect after a network change when automatic recovery is disabled.",
  "Kill switch ещё не реализован. Автовосстановление включается отдельно в настройках.":
    "Kill switch is not implemented. Automatic recovery is enabled separately in settings.",
  "При сбое ядра туннель отключается, система возвращается к обычной сети.":
    "If an engine fails, the tunnel closes and normal networking can return.",
  "Связь установлена": "Service connected",
  протокол: "protocol",
  "Для VPN нужна служба Moth с протоколом IPC 2 (0.3.1–0.3.4).":
    "VPN requires the Moth IPC 2 service (0.3.1–0.3.4); new features require 0.3.4.",
  "Проверить обновления": "Check for updates",
  "Скачать обновление": "Download update",
  "Установить обновление": "Install update",
  "Обновления приложения": "Application updates",
  "Обновление Moth": "Moth update",
  "Перезапустить приложение": "Restart application",
  "Подпись обновления не прошла проверку":
    "Update signature verification failed",
  "Файл обновления повреждён или изменён":
    "The update package is corrupt or modified",
  "Не удалось прочитать или сохранить хранилище":
    "Could not read or save protected storage",
  "Хранилище повреждено или создано другой версией":
    "The store is corrupt or from an unsupported version",
  "Защищённое хранилище недоступно; разблокируйте хранилище учётной записи":
    "Protected storage is unavailable; unlock your account keyring",
  "Подписка не найдена": "Subscription not found",
  "Подписка изменилась во время обновления":
    "The subscription changed during refresh",
  "Другое обновление подписки ещё выполняется":
    "Another subscription refresh is still running",
  "Подписка не загружена: сеть, сертификат или таймаут":
    "Subscription download failed: network, certificate or timeout",
  "Сервис подписки вернул ошибку. Последний рабочий список сохранён":
    "The subscription service returned an error. The last working list was preserved",
  "Доступ к подписке отклонён. Проверьте ссылку и регистрацию устройства у провайдера":
    "Subscription access was denied. Check the URL and device registration with your provider",
  "Провайдер сообщил о лимите устройств. Обратитесь к провайдеру":
    "The provider reported a device limit. Contact your provider",
  "Провайдер требует HWID. Разрешите его передачу этой подписке":
    "The provider requires HWID. Enable it for this subscription",
  "Для обновления нужен ваш активный VPN и служба 0.3.4 или новее":
    "Refresh requires your active VPN and service 0.3.4 or newer",
  "VPN-only: защищённый канал службы недоступен; прямой запрос запрещён":
    "VPN-only: the protected service channel is unavailable; direct requests are prohibited",
  "В ответе нет поддерживаемых серверов":
    "The response contains no supported servers",
  "Протокол в документе пока не поддерживается; обновление отменено целиком":
    "A document protocol is unsupported; the entire refresh was cancelled",
  "Документ содержит неподдерживаемые параметры сервера":
    "The document contains unsupported server parameters",
  "Параметры сервера нельзя перенести без потери; импорт отменён":
    "Server parameters cannot be preserved; import cancelled",
  "Некорректное имя сервера": "Invalid server name",
  "Небезопасный TLS запрещён": "Insecure TLS is prohibited",
  "Слишком много перенаправлений": "Too many redirects",
  "Перенаправление с HTTPS на HTTP запрещено":
    "HTTPS-to-HTTP redirects are prohibited",
  "Ответ подписки больше 2 МБ": "The subscription response exceeds 2 MB",
  "Ответ подписки не загружен полностью":
    "The subscription response was not fully downloaded",
  "Ответ подписки должен быть в UTF-8":
    "The subscription response must be UTF-8",
  "Сервер не найден": "Server not found",
  Строка: "Line",
  "Не удалось выполнить действие с VPN": "Could not perform the VPN action",
  "Служба уже выполняет действие; дождитесь завершения":
    "The service is busy; wait for it to finish",
  "Подключением управляет другая учётная запись":
    "The connection belongs to another account",
};
Object.assign(en, {
  "По одной ссылке на строку, Base64-список, Clash YAML или поддерживаемый JSON. Для URL провайдера используйте «Добавить подписку».":
    "One server link per line, a Base64 list, Clash YAML or supported JSON. Use Add subscription for a provider URL.",
  Обновления: "Updates",
  "Установка обновления": "Installing update",
  "Проверьте, доступна ли новая версия.":
    "Check whether a new version is available.",
  "Проверяем обновления…": "Checking for updates…",
  "Новых подписанных обновлений нет.": "No new signed updates.",
  "Доступна новая версия.": "A new version is available.",
  "Скачиваем и проверяем обновление…": "Downloading and verifying the update…",
  "Файл скачан, подпись проверена. Можно установить обновление.":
    "The package was downloaded and its signature verified. The update is ready to install.",
  "Отключаем VPN и запрашиваем разрешение на установку…":
    "Disconnecting VPN and requesting installation approval…",
  "Updater повторно проверяет подпись, версию и файлы…":
    "The updater is rechecking the signature, version and files…",
  "Готовим установку…": "Preparing installation…",
  "Устанавливаем приложение, службу и сетевые компоненты…":
    "Installing the application, service and network components…",
  "Установка не завершилась. Восстанавливаем предыдущую версию…":
    "Installation failed. Restoring the previous version…",
  "Обновление установлено. Открываем Moth…": "Update installed. Opening Moth…",
  "Обновление не установлено. Предыдущая версия сохранена; можно повторить попытку.":
    "The update was not installed. The previous version was preserved; you can retry.",
  "Автоматическое восстановление не подтверждено. Сохраните VPN отключённым и повторно установите полный пакет Moth.":
    "Automatic recovery is unverified. Keep VPN disconnected and reinstall the complete Moth package.",
  "Не удалось завершить обновление.": "Could not complete the update.",
  "Проверяем состояние установки…": "Checking installation status…",
  "Скачивание обновления": "Downloading update",
  МБ: "MB",
  "Открыть Moth": "Open Moth",
  "VPN будет отключён. Для установки потребуется разрешение администратора. Профили сохранятся.":
    "VPN will be disconnected. Installation requires administrator approval. Profiles will be preserved.",
  "Не удалось выполнить обновление": "Could not update",
  "Нажмите «Открыть Moth», чтобы запустить приложение.":
    "Select Open Moth to start the application.",
  "Не удалось прочитать состояние установки. Установка может продолжаться; не запускайте её повторно.":
    "Could not read installation status. Installation may still be running; do not start it again.",
  "Не удалось подтвердить отключение VPN. Moth остаётся запущен. Проверьте связь со службой и повторите выход в приложении или трее.":
    "Could not confirm VPN disconnection. Moth is still running. Check the service connection and try exiting again.",
});
en["Версии службы ещё не проверены"] =
  "Service versions have not been checked yet";
const entries = Object.entries(en).sort((a, b) => b[0].length - a[0].length);
export function translate(text: string): string {
  if (language !== "en") return text;
  for (const [from, to] of entries) text = text.split(from).join(to);
  return text;
}
export function applyPreferences(
  root: HTMLElement,
  settings?: ConnectionSettings,
) {
  language = settings?.language || "ru";
  document.documentElement.lang = language;
  const theme = settings?.theme || "system";
  document.documentElement.dataset.theme =
    theme === "system"
      ? matchMedia("(prefers-color-scheme: light)").matches
        ? "light"
        : "dark"
      : theme;
  localize(root);
}
function localize(root: HTMLElement) {
  if (language !== "en") return;
  const skip =
    "[translate=no],.selected-name,.endpoint,.server-info h3,.server-info p,.subscription-list h2";
  const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const nodes: Text[] = [];
  while (walk.nextNode()) nodes.push(walk.currentNode as Text);
  for (const node of nodes) {
    if (node.parentElement?.closest(skip)) continue;
    const value = translate(node.data);
    if (value !== node.data) node.data = value;
  }
  root
    .querySelectorAll<HTMLElement>("[aria-label],[placeholder]")
    .forEach((el) => {
      for (const attribute of ["aria-label", "placeholder"]) {
        const value = el.getAttribute(attribute);
        if (value) {
          const next = translate(value);
          if (next !== value) el.setAttribute(attribute, next);
        }
      }
    });
}
export function observeLocalization(root: HTMLElement) {
  new MutationObserver(() => localize(root)).observe(root, {
    subtree: true,
    childList: true,
    characterData: true,
  });
}
