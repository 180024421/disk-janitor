//! 与界面绘制解耦的轻量 UI 状态类型。

use crate::model::FsEntry;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tab {
    Overview,
    Browse,
    TopN,
    Types,
    Junk,
    Software,
    Startup,
    Shortcuts,
    Registry,
    Duplicates,
    Tools,
    Settings,
    About,
}

/// 「最大占用」页缓存（索引变更时失效，避免每帧全表排序）。
#[derive(Clone, Default)]
pub(crate) struct TopnCache {
    pub dirs: Vec<FsEntry>,
    pub files: Vec<FsEntry>,
    pub empty: Vec<FsEntry>,
}
