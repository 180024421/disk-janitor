# 大帅清理器 — 远程更新（对齐 DeskReader / jiaoben）

与 Reader 相同：配置 **服务器根地址**，客户端三源择优；本机额外支持 **自动下载热替换**，可选 **SHA256** 校验。

## 默认

- 根地址：`http://111.229.202.251:8687`
- appKey：`disk-janitor`

## 探测 URL

1. `{base}/disk-janitor/app-update.json`
2. `{base}/api/disk-janitor/app-update`
3. `{base}/api/app-update/disk-janitor`

支持 `{ code, data }` 包装。

## 清单字段

```json
{
  "versionCode": 4,
  "versionName": "0.3.0",
  "desktopUrl": "http://111.229.202.251:8687/disk-janitor/releases/DiskJanitor-0.3.0.exe",
  "sha256": "可选：小写 hex，下载后校验，失败则拒绝安装",
  "changelog": "0.3.0 总览/重复/工具箱等",
  "displayName": "大帅清理器",
  "enabled": true
}
```

比较规则：`versionCode` 更大，或 `versionName` 更大 → 有更新。  
若提供 `sha256`（或 `sha256sum` / `hash`），客户端下载后校验，不匹配则删除文件并报错。

## 0.4 产品原则

- **卸载残留以跟扫为主**：卸载前记下线索 → 卸载完成 →「跟扫残留」；粗扫仅作兜底且默认不勾选。
- **不扩大全盘深度扫描**；敏感垃圾规则默认不勾选并强提示。

## 客户端行为

1. 设置里检查更新 / 启动自动检查（可与扫盘并行）
2. 有新版 →「下载并安装」自动下载、校验并热替换
3. 「打开下载链接」备用

## 发版

1. 改 `Cargo.toml` 的 `version`，同步 `updater.rs` 的 `APP_VERSION_CODE`
2. 运行 `一键打包.cmd`（会尝试写入 sha256）
3. 上传 `release\DiskJanitor-x.y.z.exe`，并用 jiaoben admin 或静态文件发布 `app-update` meta
