# disk-janitor（大帅清理器）

轻量、高速的 Windows 磁盘分析与清理工具。产品名 **大帅清理器**，当前版本 **0.9.0**。

## 功能

- **总览**：各盘已用/可用；勾选多盘后「多盘排队扫描」；「极速扫描此盘」；上次扫描续扫、可回收估算、一键安全清理
- 扫描盘符 / 任意文件夹（边扫边显示）；**排除路径**（同时约束扫描与垃圾/安静清理）；取消保存断点可真续扫；CountOnly 目录可深入展开
- **极速扫描**：盘符根 + **管理员**时优先读 **NTFS MFT**；失败或不在盘根则回退 **turbo walk**（多线程、少跳过）
- 左侧目录树、排序/过滤、占比条；可选 Treemap；路径打开 / 复制
- **最大占用** / **按类型** / **垃圾建议**（显示来源、可重建性、用户内容风险及 7/30/90 天预览；高风险项默认不勾选）
- **软件卸载** + 跟扫残留与卸载前后只读差异；**查占用进程** / **查服务与计划任务**；**Store/AppX** 列表与受保护卸载
- **开机自启**：识别用途、潜在影响与「建议关闭 / 按需决定 / 建议保留」；系统项和未识别项禁止关闭
- **失效快捷方式** / **无效卸载注册表**
- **重复文件**（逐组确认、删除前 SHA256 重验、硬链接去重计量）/ **回收站** / **按扫描根历史对比** / **脱敏诊断导出**
- **安全粉碎**：确认删除时可勾选（仅文件，覆写后永久删除）
- **每日安静清理**：设置里用 `schtasks` 注册 `DiskJanitorQuietClean`，调用 `DiskJanitor.exe --quiet-clean`（无 GUI）
- **CLI**：`--quiet-clean` 安静清理后退出；`--path X` 打开时预填扫描根，追加 `--scan` 可立即开始扫描
- **主题与可访问性**：跟随系统/深色/浅色、舒适/紧凑密度、字号缩放和键盘操作
- **关于 / 赞助 / 加群**、**远程更新**（仅 HTTPS、强制 SHA256、大小限制、原子替换与失败回滚）
- 删除默认进回收站；确认框可允许直接删除 / 安全粉碎

## 明确边界

- **MFT**：需管理员且目标为盘符根（如 `C:\`）；否则自动 turbo walk
- **壳扩展**：不提供 DLL；见 [deploy/explorer-context-menu.reg.example](deploy/explorer-context-menu.reg.example)
- **托盘常驻**：未做完整托盘；后台清理用计划任务 + `--quiet-clean`
- **多语言**：当前以中文界面为主
- **媒体元数据**：图片可显示尺寸；当前不猜测音视频时长

## 运行

```powershell
cd D:\project\disk-janitor
.\start.cmd
```

安静清理（无界面）：

```powershell
.\target\release\disk-janitor.exe --quiet-clean
```

当前用户安装/卸载 Explorer 右键菜单（无需管理员）：

```powershell
.\scripts\install-explorer-menu.ps1 -ExecutablePath .\target\release\disk-janitor.exe
.\scripts\uninstall-explorer-menu.ps1
```

## 打包发版

便携版（含热更新清单）：

```powershell
.\pack.cmd
# 或双击 一键打包.cmd
```

可安装 Setup（产品名「大帅清理器」，写入开始菜单 / 桌面 / 控制面板卸载项）：

```powershell
.\pack-setup.cmd
# 或双击 一键安装包.cmd
```

产物：
- 便携：`release\DiskJanitor-x.y.z.exe` + `deploy\app-update.json`
- 安装包：`release\大帅清理器-Setup-x.y.z.exe`（同内容 `DashuaiCleaner-Setup-x.y.z.exe`）

热更新仍指向便携 exe；安装包元数据见 `deploy\setup-release.json`。说明见 [docs/REMOTE-UPDATE.md](docs/REMOTE-UPDATE.md)。

可选：若本机已装 [Inno Setup](https://jrsoftware.org/isinfo.php)，也可用 `installer\dashuai-cleaner.iss` 另行编译。

## 测试

```powershell
cargo test --release -q
```
