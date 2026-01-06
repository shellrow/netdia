use crate::model::SysInfo;
use crate::net::sys::os::system_info;

#[tauri::command]
pub fn get_sys_info() -> SysInfo {
    system_info()
}
