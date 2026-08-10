//! 管理员提权重启

use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn is_elevated() -> bool {
    // 尝试打开需管理员的服务控制管理器作启发式判断
    #[link(name = "advapi32")]
    extern "system" {
        fn OpenSCManagerW(machine: *const u16, database: *const u16, access: u32) -> isize;
        fn CloseServiceHandle(h: isize) -> i32;
    }
    const SC_MANAGER_LOCK: u32 = 0x0008;
    let h = unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_LOCK) };
    if h != 0 {
        unsafe {
            CloseServiceHandle(h);
        }
        true
    } else {
        false
    }
}

/// 以管理员身份重新启动当前 exe，成功后调用方应退出
pub fn relaunch_as_admin() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_s = exe.to_string_lossy().replace('\'', "''");
    let script = format!(
        "Start-Process -FilePath '{exe_s}' -Verb RunAs"
    );
    let status = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("用户取消了提权，或系统拒绝启动".into())
    }
}
