//! 大帅清理器安装程序：解压内嵌 payload.zip → Program Files，写快捷方式与卸载项

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const APP_NAME: &str = "大帅清理器";
const APP_EXE: &str = "大帅清理器.exe";
const APP_KEY: &str = "DashuaiCleaner";
const VERSION: &str = env!("CARGO_PKG_VERSION");

// 由 一键安装包.cmd 生成 installer/payload.zip 后再编译本程序
const PAYLOAD: &[u8] = include_bytes!("payload.zip");

fn main() {
    if !is_elevated() {
        eprintln!("需要管理员权限，正在提权…");
        if relaunch_elevated() {
            return;
        }
        message("请右键「以管理员身份运行」安装程序。");
        std::process::exit(1);
    }

    let install_dir = default_install_dir();
    let msg = format!(
        "即将安装 {APP_NAME} v{VERSION}\n\n安装目录：\n{}\n\n是否继续？",
        install_dir.display()
    );
    if !confirm(&msg) {
        return;
    }

    if let Err(e) = install_to(&install_dir) {
        message(&format!("安装失败：{e}"));
        std::process::exit(1);
    }

    let launch = confirm(&format!("{APP_NAME} 安装完成！\n\n是否立即运行？"));
    if launch {
        let exe = install_dir.join(APP_EXE);
        let _ = Command::new(&exe).current_dir(&install_dir).spawn();
    }
}

fn default_install_dir() -> PathBuf {
    let pf = std::env::var_os("ProgramFiles").unwrap_or_else(|| r"C:\Program Files".into());
    PathBuf::from(pf).join(APP_NAME)
}

fn install_to(dir: &Path) -> Result<(), String> {
    if dir.exists() {
        let _ = fs::remove_dir_all(dir);
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;

    let tmp_zip = std::env::temp_dir().join(format!("dashuai-cleaner-payload-{VERSION}.zip"));
    {
        let mut f = File::create(&tmp_zip).map_err(|e| e.to_string())?;
        f.write_all(PAYLOAD).map_err(|e| e.to_string())?;
    }
    extract_zip(&tmp_zip, dir)?;
    let _ = fs::remove_file(&tmp_zip);

    let main_exe = dir.join(APP_EXE);
    if !main_exe.exists() {
        let fallback = dir.join("disk-janitor.exe");
        if fallback.exists() {
            fs::copy(&fallback, &main_exe).map_err(|e| e.to_string())?;
        } else {
            return Err("安装包缺少主程序".into());
        }
    }

    write_uninstall_script(dir)?;
    create_shortcuts(dir)?;
    register_uninstall(dir)?;
    Ok(())
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let outpath = match file.enclosed_name() {
            Some(p) => dest.join(p),
            None => continue,
        };
        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = outpath.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = File::create(&outpath).map_err(|e| e.to_string())?;
            io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn write_uninstall_script(dir: &Path) -> Result<(), String> {
    let uninst = dir.join("卸载大帅清理器.cmd");
    let dir_s = dir.display().to_string();
    let body = format!(
        "@echo off\r\n\
         chcp 65001 >nul\r\n\
         echo 正在卸载 {APP_NAME}…\r\n\
         reg delete \"HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_KEY}\" /f >nul 2>&1\r\n\
         del /f /q \"%USERPROFILE%\\Desktop\\{APP_NAME}.lnk\" >nul 2>&1\r\n\
         del /f /q \"%APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\{APP_NAME}.lnk\" >nul 2>&1\r\n\
         cd /d \"%TEMP%\"\r\n\
         start \"\" cmd /c \"timeout /t 2 /nobreak >nul & rmdir /s /q \"\"{dir_s}\"\"\"\r\n"
    );
    fs::write(uninst, body).map_err(|e| e.to_string())?;
    Ok(())
}

fn create_shortcuts(dir: &Path) -> Result<(), String> {
    let exe = dir.join(APP_EXE);
    let ps = format!(
        "$ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut([IO.Path]::Combine($env:APPDATA, 'Microsoft\\Windows\\Start Menu\\Programs', '{APP_NAME}.lnk')); \
         $s.TargetPath = '{}'; $s.WorkingDirectory = '{}'; $s.Description = '{APP_NAME}'; $s.Save(); \
         $d = $ws.CreateShortcut([IO.Path]::Combine($env:USERPROFILE, 'Desktop', '{APP_NAME}.lnk')); \
         $d.TargetPath = '{}'; $d.WorkingDirectory = '{}'; $d.Description = '{APP_NAME}'; $d.Save();",
        exe.display(),
        dir.display(),
        exe.display(),
        dir.display()
    );
    let status = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &ps])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("创建快捷方式失败".into());
    }
    Ok(())
}

fn register_uninstall(dir: &Path) -> Result<(), String> {
    let uninst = dir.join("卸载大帅清理器.cmd");
    let exe = dir.join(APP_EXE);
    let display_icon = format!("{},0", exe.display());
    let cmds = [
        format!(
            "reg add \"HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_KEY}\" /v DisplayName /t REG_SZ /d \"{APP_NAME}\" /f"
        ),
        format!(
            "reg add \"HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_KEY}\" /v DisplayVersion /t REG_SZ /d \"{VERSION}\" /f"
        ),
        format!(
            "reg add \"HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_KEY}\" /v Publisher /t REG_SZ /d \"lidashuai\" /f"
        ),
        format!(
            "reg add \"HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_KEY}\" /v InstallLocation /t REG_SZ /d \"{}\" /f",
            dir.display()
        ),
        format!(
            "reg add \"HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_KEY}\" /v DisplayIcon /t REG_SZ /d \"{display_icon}\" /f"
        ),
        format!(
            "reg add \"HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_KEY}\" /v UninstallString /t REG_SZ /d \"\\\"{}\\\"\" /f",
            uninst.display()
        ),
        format!(
            "reg add \"HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_KEY}\" /v NoModify /t REG_DWORD /d 1 /f"
        ),
        format!(
            "reg add \"HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{APP_KEY}\" /v NoRepair /t REG_DWORD /d 1 /f"
        ),
    ];
    for c in cmds {
        let _ = Command::new("cmd").args(["/C", &c]).status();
    }
    Ok(())
}

fn is_elevated() -> bool {
    Command::new("net")
        .args(["session"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn relaunch_elevated() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let ps = format!(
        "Start-Process -FilePath '{}' -Verb RunAs -Wait",
        exe.display()
    );
    Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn message(text: &str) {
    let _ = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show('{}','{APP_NAME}') | Out-Null",
                text.replace('\'', "''")
            ),
        ])
        .status();
}

fn confirm(text: &str) -> bool {
    let script = format!(
        "Add-Type -AssemblyName PresentationFramework; $r = [System.Windows.MessageBox]::Show('{}','{APP_NAME}','YesNo','Question'); if ($r -eq 'Yes') {{ exit 0 }} else {{ exit 1 }}",
        text.replace('\'', "''").replace('\n', "`n")
    );
    Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
