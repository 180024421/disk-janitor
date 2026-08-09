# disk-janitor — jiaoben 远程更新 + 界面改版

日期：2026-08-09  
状态：已确认（方案 B）

## 更新

- 配置：`update_api_base`（默认 `http://111.229.202.251:8687`），不再手填 `latest.json`
- 三源择优：`/{app}/app-update.json`、`/api/{app}/app-update`、`/api/app-update/{app}`，`appKey=disk-janitor`
- 字段：`versionCode` + `versionName` + `desktopUrl` + `changelog`（兼容 Reader 别名与 `{data}` 包装）
- 比较：`versionCode` 更大 **或** `versionName` 更大 → 有更新
- 应用：自动下载 exe 并热替换（与 Reader 唯一差别）；设置页另提供「打开下载链接」

## 界面

- 深色岩灰 + 青绿强调，顶栏品牌 / 版本胶囊 / 分段 Tab
- 分区卡片、主按钮与幽灵按钮层级分明

## 打包

- `一键打包.cmd` 产出 exe + `deploy/app-update.json`（含 versionCode）
