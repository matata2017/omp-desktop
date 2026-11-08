# UI audit

Generated 2026-11-08T10:16:40.631Z from the locale files and a running window.

## Copy findings

Sorted by how often the key is used in `src/renderer`. A hit list, not a rewrite order.

| uses | locale | key | hits | text |
|---:|---|---|---|---|
| 2 | zh.ts | `settings.editors.kvDuplicateKey` | curly quote | 已有条目使用“{key}”，请给这一行换个名字。 |
| 1 | zh.ts | `advisor.noModel` | curly quote | 顾问设置已启用，但尚未为“advisor”角色分配模型。 |
| 1 | en.ts | `agentHub.defs.modelPlaceholder` | em dash | provider/model or pattern — empty clears |
| 1 | zh.ts | `agentHub.defs.modelPlaceholder` | em dash | provider/模型 或模式 — 留空则清除 |
| 1 | ru.ts | `agentHub.defs.modelPlaceholder` | em dash | провайдер/модель или шаблон — оставьте пустым, чтобы очистить |
| 1 | zh.ts | `agents.roster.turnEmptyBody` | curly quote | 之前几轮的代理在“全部”里。 |
| 1 | ru.ts | `agents.roster.turnEmptyBody` | em dash | Агенты прошлых ходов — во вкладке «Все». |
| 1 | zh.ts | `branchPicker.noMatch` | curly quote | 没有匹配“{query}”的消息。 |
| 1 | en.ts | `chat.awaitingModel.slow` | em dash | Slow response — the provider or network may be struggling |
| 1 | ru.ts | `chat.awaitingModel.slow` | em dash | Медленный ответ — возможно, у провайдера или сети проблемы |
| 1 | zh.ts | `chat.awaitingModel.stalled` | em dash | 仍无响应——请求约 5 分钟后会超时并自动重试。 |
| 1 | ru.ts | `chat.context.compactedFrom` | em dash | Контекст сжат: до сжатия — {tokens} токенов |
| 1 | zh.ts | `chat.empty.subtitle.chat` | em dash | 随便问——无工具的纯对话，上下文与压缩照常。不能读文件、不能跑命令，只有回答。 |
| 1 | en.ts | `chat.retry.announcement` | em dash | Request failed — retrying, attempt {attempt} of {maxAttempts} |
| 1 | ru.ts | `chat.retry.announcement` | em dash | Запрос не выполнен — повторная попытка {attempt} из {maxAttempts} |
| 1 | en.ts | `cmd.pin.noFile` | em dash | Send a message first — the session has no file to pin yet. |
| 1 | zh.ts | `cmd.pin.noFile` | em dash | 先发送一条消息——会话还没有可置顶的文件。 |
| 1 | ru.ts | `cmd.pin.noFile` | em dash | Сначала отправьте сообщение — у сессии ещё нет файла для закрепления. |
| 1 | en.ts | `cmd.trace.noSession` | em dash | Send a message first — the session has no file yet. |
| 1 | zh.ts | `cmd.trace.noSession` | em dash | 先发送一条消息——会话还没有文件。 |
| 1 | ru.ts | `cmd.trace.noSession` | em dash | Сначала отправьте сообщение — у сессии ещё нет файла. |
| 1 | zh.ts | `contextStrategy.strategy.custom.desc` | em dash | 你自己的方法顺序或保留量 — 可在「全部压缩设置」中修改。 |
| 1 | zh.ts | `dag.unresolved` | em dash | {count} 个嵌套代理显示在根层级 — 父链接不可用；展开记录可解析嵌套关系。 |
| 1 | zh.ts | `debug.noSessionsHint` | em dash | 启动程序或附加到进程即可开始调试——在下方填入示例请求，或让代理代为启动调试。 |
| 1 | en.ts | `debug.outcome.timeout` | em dash | Program is still running — use Pause to interrupt and inspect. |
| 1 | zh.ts | `debug.outcome.timeout` | em dash | 程序仍在运行——可用「暂停」中断并检查状态。 |
| 1 | en.ts | `editor.discardArmed` | em dash | Unsaved changes — press Esc again to discard |
| 1 | zh.ts | `editor.discardArmed` | em dash | 有未保存的修改 — 再按 Esc 丢弃 |
| 1 | ru.ts | `editor.discardArmed` | em dash | Есть несохранённые изменения — нажми Esc ещё раз, чтобы отменить их |
| 1 | en.ts | `extPanel.noMatch` | curly quote | No matches for “{query}”. |
| 1 | zh.ts | `extPanel.noMatch` | curly quote | 没有匹配“{query}”的结果。 |
| 1 | en.ts | `feedback.includeErrors` | em dash | Include recent error log (application errors only — never conversation content) |
| 1 | ru.ts | `feedback.includeErrors` | em dash | Включить недавний журнал ошибок (только ошибки приложения — переписка не включается) |
| 1 | en.ts | `feedback.summaryPlaceholder` | em dash | Short summary (optional — derived from the description if empty) |
| 1 | ru.ts | `feedback.summaryPlaceholder` | em dash | Краткое описание (необязательно — будет составлено по тексту, если поле пустое) |
| 1 | en.ts | `filesPanel.binary` | em dash | (binary file — preview unavailable) |
| 1 | zh.ts | `filesPanel.binary` | em dash | （二进制文件 — 无法预览） |
| 1 | ru.ts | `filesPanel.binary` | em dash | (двоичный файл — предпросмотр недоступен) |
| 1 | zh.ts | `handoff.blockedStreaming` | em dash | 代理正在响应时无法交接 — 请等待其完成或先中止。 |
| 1 | ru.ts | `hotkeys.remap.captureHint` | em dash | Esc — отмена |
| 1 | en.ts | `hotkeys.remap.conflictShadow` | curly quote | Shadows the default for “{action}” |
| 1 | en.ts | `hotkeys.remap.conflictUser` | curly quote | Already bound to “{action}” |
| 1 | en.ts | `hotkeys.remap.rebinding` | curly quote | Rebinding “{action}” |
| 1 | en.ts | `hotkeys.remap.resetDone` | curly quote | “{action}” reset to default |
| 1 | en.ts | `hotkeys.remap.saved` | curly quote | “{action}” rebound to {chord} |
| 1 | en.ts | `import.copyNote` | em dash | Importing creates a copy — the original data is never modified. |
| 1 | zh.ts | `import.copyNote` | em dash | 导入是拷贝操作 — 不会修改原始数据。 |
| 1 | ru.ts | `import.copyNote` | em dash | Импорт создаёт копию — исходные данные не изменяются. |
| 1 | en.ts | `input.approval.yoloConfirmBody` | em dash | Every tool call — file writes, commands, network — will run without asking. |
| 1 | en.ts | `input.goal.activeTitle` | em dash | Goal mode — {objective} |
| 1 | zh.ts | `input.goal.activeTitle` | em dash | 目标模式 — {objective} |
| 1 | ru.ts | `input.goal.activeTitle` | em dash | Режим цели — {objective} |
| 1 | en.ts | `input.goal.title` | em dash | Goal mode — open the goal panel |
| 1 | zh.ts | `input.goal.title` | em dash | 目标模式 — 打开目标面板 |
| 1 | ru.ts | `input.goal.title` | em dash | Режим цели — открыть панель целей |
| 1 | en.ts | `input.loop.activeTitle` | em dash | Loop mode — {args} |
| 1 | zh.ts | `input.loop.activeTitle` | em dash | 循环模式 — {args} |
| 1 | ru.ts | `input.loop.activeTitle` | em dash | Режим цикла — {args} |
| 1 | en.ts | `input.loop.title` | em dash | Loop mode — open the loop panel |
| 1 | zh.ts | `input.loop.title` | em dash | 循环模式 — 打开循环面板 |
| 1 | ru.ts | `input.loop.title` | em dash | Режим цикла — открыть панель цикла |
| 1 | en.ts | `input.mode.bash.title` | em dash | Bash mode — Enter runs locally via the shell (!cmd) |
| 1 | zh.ts | `input.mode.bash.title` | em dash | Bash 模式 — Enter 通过 shell 本地运行 (!cmd) |
| 1 | ru.ts | `input.mode.bash.title` | em dash | Режим Bash — Enter запускает команду локально через Shell (!cmd) |
| 1 | en.ts | `input.mode.python.title` | em dash | Python mode — runs via the agent's python tool ($ code) |
| 1 | zh.ts | `input.mode.python.title` | em dash | Python 模式 — 通过代理的 python 工具运行 ($ code) |
| 1 | ru.ts | `input.mode.python.title` | em dash | Режим Python — запуск через инструмент Python агента ($ code) |
| 1 | ru.ts | `input.paste.hint` | em dash | Esc — отмена |
| 1 | en.ts | `input.placeholder.chat` | em dash | Ask anything — no tools, just conversation… |
| 1 | zh.ts | `input.placeholder.chat` | em dash | 随便问——无工具，纯对话… |
| 1 | ru.ts | `input.placeholder.chat` | em dash | Спроси что угодно — только переписка, без инструментов… |
| 1 | en.ts | `input.placeholder.streaming` | em dash | Add guidance while omp is working — {chord} queues a follow-up… |
| 1 | zh.ts | `input.placeholder.streaming` | em dash | 在 omp 工作时添加引导 — {chord} 排队跟进… |
| 1 | ru.ts | `input.placeholder.streaming` | em dash | Добавь указания, пока omp выполняет задачу — {chord} ставит дополнение в очередь… |
| 1 | en.ts | `input.plan.title` | em dash | Plan mode — agent drafts a plan before acting |
| 1 | zh.ts | `input.plan.title` | em dash | 计划模式 — 先出计划再动手 |
| 1 | ru.ts | `input.plan.title` | em dash | Режим планирования — агент составляет план перед выполнением |
| 1 | en.ts | `input.queue.title` | em dash | Starts with -> / => — sent after the agent yields |
| 1 | zh.ts | `input.queue.title` | em dash | 以 -> / => 开头 — 代理交出控制权后发送 |
| 1 | ru.ts | `input.queue.title` | em dash | Начни с -> / => — сообщение отправится, когда агент освободится |
| 1 | en.ts | `input.roles.title` | em dash | Model roles — assign models per role |
| 1 | zh.ts | `input.roles.title` | em dash | 模型角色 — 按角色分配模型 |
| 1 | ru.ts | `input.roles.title` | em dash | Роли моделей — назначить модели для каждой роли |
| 1 | en.ts | `input.thinking` | em dash | Thinking: {level} — click to change |
| 1 | zh.ts | `input.thinking` | em dash | 思考级别：{level} — 点击更改 |
| 1 | ru.ts | `input.thinking` | em dash | Thinking: {level} — нажми, чтобы изменить |
| 1 | en.ts | `invPanel.readonlyNote` | em dash | Read-only — install and removal actions will appear here in a future update. |
| 1 | zh.ts | `invPanel.readonlyNote` | em dash | 只读 —— 安装与移除操作将在后续版本中提供。 |
| 1 | ru.ts | `invPanel.readonlyNote` | em dash | Только чтение — установка и удаление появятся здесь в будущем обновлении. |
| 1 | ru.ts | `links.webHint` | em dash | Щелчок — открыть в панели браузера · {mod}+щелчок — открыть в вашем браузере |
| 1 | ru.ts | `links.webHintExternal` | em dash | Щелчок — открыть в вашем браузере |
| 1 | en.ts | `live.ready` | em dash | Ready — microphone is off |
| 1 | ru.ts | `live.ready` | em dash | Готово — микрофон выключен |
| 1 | zh.ts | `marketplaceAction.badId` | curly quote | 需要形如 name@marketplace 的插件 ID（收到“{id}”） |
| 1 | en.ts | `mcp.reauth.running` | em dash | Authorization in progress — finish the sign-in flow in the dialog. |
| 1 | zh.ts | `mcp.reauth.running` | em dash | 授权进行中——请在弹出的对话框中完成登录。 |
| 1 | ru.ts | `mcp.reauth.running` | em dash | Идёт авторизация — завершите вход в открывшемся диалоговом окне. |
| 1 | en.ts | `mcp.test.ok` | em dash | Connection OK — {count} tools |
| 1 | zh.ts | `mcp.test.ok` | em dash | 连接正常——{count} 个工具 |
| 1 | ru.ts | `mcp.test.ok` | em dash | Подключение установлено — инструментов: {count} |

Total findings: 328. Zero-use keys are likely stale; check before editing.

## Accessibility

| impact | id | nodes | help |
|---|---|---:|---|
| serious | nested-interactive | 3 | Interactive controls must not be nested |

Screenshot: `docs/ui-audit/screen.png`.
