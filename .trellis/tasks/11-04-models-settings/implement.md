# Implement：Models, settings, themes, i18n

1. 模型目录/选择/切换/思考级别/fast。
2. provider 认证、自定义 models.yml、角色、用量/配额。
3. `get_settings_schema` 驱动页 + GUI 偏好 + 审批/运行时开关。
4. 管理窗口：Modes/Extensions/Inventory/Compare/Hotkeys/Onboarding。
5. 主题与 i18n（含原生菜单/托盘文案）。
6. parity 账本逐行推进。

验证：切换模型/角色、设置即时生效、主题/语言切换、provider 登录。

回滚：按域提交；配置写入保持原子。
