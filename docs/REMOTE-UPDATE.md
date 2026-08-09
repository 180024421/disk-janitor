# 磁盘管家 — 远程更新（对齐 DeskReader / jiaoben）

与 Reader 相同：配置 **服务器根地址**，客户端三源择优；本机额外支持 **自动下载热替换**。

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
  "versionCode": 3,
  "versionName": "0.2.1",
  "desktopUrl": "http://111.229.202.251:8687/disk-janitor/releases/DiskJanitor-0.2.1.exe",
  "changelog": "jiaoben 更新 + 界面改版",
  "displayName": "磁盘管家",
  "enabled": true
}
```

比较规则：`versionCode` 更大，或 `versionName` 更大 → 有更新。

## 客户端行为（方案 B）

1. 设置里检查更新 / 启动自动检查  
2. 有新版 →「下载并安装」自动下载并热替换  
3. 「打开下载链接」备用（与 Reader 一致）

## 发版

1. 改 `Cargo.toml` 的 `version`，同步 `updater.rs` 的 `APP_VERSION_CODE`  
2. 运行 `一键打包.cmd`  
3. 上传 `release\DiskJanitor-x.y.z.exe`，并用 jiaoben admin 或静态文件发布 `app-update` meta
