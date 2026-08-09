# disk-janitor（磁盘管家）

轻量、高速的 Windows 磁盘分析与清理工具。当前版本 **0.2.1**。

## 功能

- 扫描盘符 / 任意文件夹，显示**真实占用大小**（边扫边显示）
- 左侧目录树、按大小/时间/名称排序、名称过滤
- **最大占用** Top 40 文件/文件夹 + **空文件夹**列表
- **垃圾建议** / **软件卸载** / **开机自启** / **失效快捷方式** / **无效卸载注册表**
- **远程更新**：对齐 DeskReader 的 jiaoben `app-update`（三源择优），本机可自动下载热替换
- 文件删除默认 **移到回收站**；敏感路径二次确认

## 运行

```powershell
cd D:\project\disk-janitor
.\start.cmd
```

## 打包发版

```powershell
.\一键打包.cmd
```

产物：`release\DiskJanitor-x.y.z.exe` + `deploy\app-update.json`。说明见 [docs/REMOTE-UPDATE.md](docs/REMOTE-UPDATE.md)。

## 测试

```powershell
cargo test --release
```
