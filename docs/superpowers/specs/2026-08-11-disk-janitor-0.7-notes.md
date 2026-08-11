# disk-janitor 0.7.0 说明

## 本版要点

| 能力 | 实现 |
|------|------|
| 排除列表 | `AppConfig.exclude_paths` + 扫描前缀跳过 |
| 极速扫描 | `scan_mode=turbo` / `fast_scan`：少 CountOnly、提高估算上限 |
| 多盘排队 | 总览勾选 → 顺序扫描 |
| 深度卸载 | PowerShell：占用进程 / 服务 / 任务 / AppX |
| 计划任务 | `schtasks` → `--quiet-clean` |
| 垃圾规则 | Windows.old、CBS、FontCache、DX、隐私 Cookies/历史 |
| 安全粉碎 | `shred_file` + 删除确认勾选 |
| 右键菜单 | 仅 `.reg.example`，无壳扩展 DLL |
| 托盘 | 未做；用计划任务代替 |

## 诚实边界

- 极速 = **turbo walk**，不是管理员 MFT 枚举
- 无完整托盘常驻
- 壳扩展用注册表示例调用 `--path`
