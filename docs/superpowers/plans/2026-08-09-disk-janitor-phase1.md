# disk-janitor 一期 Implementation Plan

> **For agentic workers:** Implement task-by-task. Steps use checkbox syntax.

**Goal:** Windows 轻量磁盘分析器：扫盘、按大小/时间排序、移到回收站。

**Architecture:** eframe UI + 后台扫盘线程 + trash 回收站。

**Tech Stack:** Rust, eframe/egui, rayon, trash, walkdir/jwalk

## Global Constraints

- Windows 优先；删除默认进回收站
- UI 中文；Release 可运行
- 一期不做垃圾规则 / 重复文件 / 开机项

---

### Task 1: 工程骨架

- [ ] `cargo init`，依赖：eframe, egui, rayon, trash, chrono
- [ ] README 启动说明
- [ ] `cargo build` 通过

### Task 2: 扫盘引擎

- [ ] `FsNode` 树（name, path, size, mtime, is_dir, children）
- [ ] 并行扫描；进度 channel；跳过无权目录
- [ ] 单测：临时目录聚合大小

### Task 3: UI

- [ ] 盘符/路径、进度、树+列表、排序、过滤
- [ ] 多选与合计大小

### Task 4: 回收站删除

- [ ] 确认框；系统路径警告；`trash::delete`

### Task 5: 验证

- [ ] `cargo test` + `cargo build --release`
- [ ] 手工：扫用户目录、排序、删测试文件进回收站
