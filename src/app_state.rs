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

/// 左侧导航的一级入口。Tab 仍是视图的唯一真源，切段选不会丢各视图自己的状态。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    Home,
    Space,
    Clean,
    System,
    Config,
}

pub(crate) const SECTIONS: [Section; 5] = [
    Section::Home,
    Section::Space,
    Section::Clean,
    Section::System,
    Section::Config,
];

impl Section {
    pub(crate) fn of(tab: Tab) -> Section {
        match tab {
            Tab::Overview => Section::Home,
            Tab::Browse | Tab::TopN | Tab::Types => Section::Space,
            Tab::Junk | Tab::Duplicates | Tab::Tools => Section::Clean,
            Tab::Software | Tab::Startup | Tab::Shortcuts | Tab::Registry => Section::System,
            Tab::Settings | Tab::About => Section::Config,
        }
    }

    pub(crate) fn icon(self) -> &'static str {
        match self {
            Section::Home => "🏠",
            Section::Space => "📁",
            // 不用 🧹：egui 内置的 emoji 字体没有这个码位，会渲染成豆腐块。
            Section::Clean => "🗑",
            Section::System => "📦",
            Section::Config => "⚙",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Section::Home => "体检",
            Section::Space => "空间分析",
            Section::Clean => "清理",
            Section::System => "系统",
            Section::Config => "设置",
        }
    }

    pub(crate) fn hint(self) -> &'static str {
        match self {
            Section::Home => "各盘空间与清理建议",
            Section::Space => "谁占用了磁盘",
            Section::Clean => "垃圾、重复文件与工具箱",
            Section::System => "应用、启动项与残留",
            Section::Config => "偏好、授权与关于",
        }
    }

    /// 页内段选：单项的分组不显示段选。
    pub(crate) fn views(self) -> &'static [(Tab, &'static str)] {
        match self {
            Section::Home => &[],
            Section::Space => &[
                (Tab::Browse, "目录树"),
                (Tab::TopN, "最大占用"),
                (Tab::Types, "按类型"),
            ],
            Section::Clean => &[
                (Tab::Junk, "垃圾建议"),
                (Tab::Duplicates, "重复文件"),
                (Tab::Tools, "工具箱"),
            ],
            Section::System => &[
                (Tab::Software, "应用"),
                (Tab::Startup, "启动项"),
                (Tab::Shortcuts, "快捷方式"),
                (Tab::Registry, "注册表"),
            ],
            // 段选标签不叫「设置」：一级标题已经是「设置」，同名会让「关于」页看起来像标题错了。
            Section::Config => &[(Tab::Settings, "偏好"), (Tab::About, "关于")],
        }
    }

    pub(crate) fn first_tab(self) -> Tab {
        match self.views() {
            [] => Tab::Overview,
            v => v[0].0,
        }
    }
}

/// 「最大占用」页缓存（索引变更时失效，避免每帧全表排序）。
#[derive(Clone, Default)]
pub(crate) struct TopnCache {
    pub dirs: Vec<FsEntry>,
    pub files: Vec<FsEntry>,
    pub empty: Vec<FsEntry>,
}
