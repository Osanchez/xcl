//! An NVIDIA driver profile (DRS) for an executable, written through NVAPI (#2403).
//!
//! NVIDIA's D3D12 driver reserves 512 MB of VA for the machine code of the pipelines in use and
//! removes the device when a submission needs more. Setting `0x001ABC0F` ("the amount of VA space
//! in bytes reserved for the shader heap") in a profile for the executable raises it; NVIDIA's own
//! profiles use 1 or 2 GiB. This writes that one setting into a profile named [`PROFILE`] that
//! holds only XCL's executables, so [`remove`] takes it out again without touching anything else.

use std::ffi::c_void;

/// The setting: VA space in bytes reserved for the shader heap (a multiple of 64 KiB).
pub const SHADER_HEAP_VA: u32 = 0x001A_BC0F;
/// The profile XCL owns.
pub const PROFILE: &str = "XCL (xcl-dev)";

const UNICODE_LEN: usize = 2048;
type Wide = [u16; UNICODE_LEN];
type Handle = *mut c_void;

#[repr(C)]
struct Profile {
    version: u32,
    name: Wide,
    gpu_support: u32,
    is_predefined: u32,
    num_apps: u32,
    num_settings: u32,
}

#[repr(C)]
struct Application {
    version: u32,
    is_predefined: u32,
    app_name: Wide,
    friendly_name: Wide,
    launcher: Wide,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Binary {
    len: u32,
    data: [u8; 4096],
}

#[repr(C)]
#[derive(Clone, Copy)]
union Value {
    u32: u32,
    binary: Binary,
    wide: Wide,
}

#[repr(C)]
struct Setting {
    version: u32,
    name: Wide,
    id: u32,
    kind: u32,
    location: u32,
    is_current_predefined: u32,
    is_predefined_valid: u32,
    predefined: Value,
    current: Value,
}

const fn ver<T>(v: u32) -> u32 {
    std::mem::size_of::<T>() as u32 | (v << 16)
}

fn wide(s: &str) -> Wide {
    let mut w = [0u16; UNICODE_LEN];
    for (d, c) in w.iter_mut().zip(s.encode_utf16().take(UNICODE_LEN - 1)) {
        *d = c;
    }
    w
}

fn zeroed<T>() -> Box<T> {
    // SAFETY: every type here is plain integers / arrays, for which all-zero is valid.
    unsafe { Box::new(std::mem::zeroed()) }
}

type Status = i32;
const OK: Status = 0;
const INVALID_USER_PRIVILEGE: Status = -137;
const SETTING_NOT_FOUND: Status = -160;
const PROFILE_NOT_FOUND: Status = -163;
const EXECUTABLE_NOT_FOUND: Status = -166; // values from NVIDIA/nvapi nvapi_lite_common.h

struct Api {
    create_session: unsafe extern "C" fn(*mut Handle) -> Status,
    destroy_session: unsafe extern "C" fn(Handle) -> Status,
    load: unsafe extern "C" fn(Handle) -> Status,
    save: unsafe extern "C" fn(Handle) -> Status,
    find_app: unsafe extern "C" fn(Handle, *const u16, *mut Handle, *mut Application) -> Status,
    find_profile: unsafe extern "C" fn(Handle, *const u16, *mut Handle) -> Status,
    create_profile: unsafe extern "C" fn(Handle, *mut Profile, *mut Handle) -> Status,
    delete_profile: unsafe extern "C" fn(Handle, Handle) -> Status,
    create_app: unsafe extern "C" fn(Handle, Handle, *mut Application) -> Status,
    set_setting: unsafe extern "C" fn(Handle, Handle, *mut Setting) -> Status,
    get_setting: unsafe extern "C" fn(Handle, Handle, u32, *mut Setting) -> Status,
}

fn api() -> Result<Api, String> {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
    unsafe {
        // System32 only: `xcl nvprofile install` runs elevated, so the executable's own folder is not searched
        let lib = LoadLibraryExW(windows::core::w!("nvapi64.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32).map_err(|e| format!("no nvapi64.dll in System32: {e}"))?;
        let qi = GetProcAddress(lib, windows::core::s!("nvapi_QueryInterface")).ok_or("no nvapi_QueryInterface")?;
        let qi: unsafe extern "C" fn(u32) -> *const c_void = std::mem::transmute(qi);
        let get = |id: u32| {
            let p = qi(id);
            if p.is_null() { Err(format!("NVAPI has no function {id:#010x}")) } else { Ok(p) }
        };
        let init: unsafe extern "C" fn() -> Status = std::mem::transmute(get(0x0150_E828)?);
        let s = init();
        if s != OK {
            return Err(format!("NvAPI_Initialize: {s}"));
        }
        Ok(Api {
            create_session: std::mem::transmute(get(0x0694_D52E)?),
            destroy_session: std::mem::transmute(get(0xDAD9_CFF8)?),
            load: std::mem::transmute(get(0x375D_BD6B)?),
            save: std::mem::transmute(get(0xFCBC_7E14)?),
            find_app: std::mem::transmute(get(0xEEE5_66B2)?),
            find_profile: std::mem::transmute(get(0x7E4A_9A0B)?),
            create_profile: std::mem::transmute(get(0xCC17_6068)?),
            delete_profile: std::mem::transmute(get(0x1709_3206)?),
            create_app: std::mem::transmute(get(0x4347_A9DE)?),
            set_setting: std::mem::transmute(get(0x577D_D202)?),
            get_setting: std::mem::transmute(get(0x73BF_8338)?),
        })
    }
}

struct Session<'a> {
    api: &'a Api,
    h: Handle,
}

impl<'a> Session<'a> {
    fn open(api: &'a Api) -> Result<Self, String> {
        let mut h = std::ptr::null_mut();
        unsafe {
            check("CreateSession", (api.create_session)(&mut h))?;
            let s = Session { api, h };
            check("LoadSettings", (api.load)(h))?;
            Ok(s)
        }
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        unsafe { (self.api.destroy_session)(self.h) };
    }
}

fn check(what: &str, s: Status) -> Result<(), String> {
    if s == OK { Ok(()) } else { Err(format!("NvAPI_DRS_{what}: {s}")) }
}

/// A lookup's result: `true` when found, `false` only for NVAPI's not-found codes, and an error for
/// anything else (an ambiguous executable path, a failed call), so a failure is not read as absence.
fn found(what: &str, s: Status) -> Result<bool, String> {
    match s {
        OK => Ok(true),
        SETTING_NOT_FOUND | PROFILE_NOT_FOUND | EXECUTABLE_NOT_FOUND => Ok(false),
        _ => Err(format!("NvAPI_DRS_{what}: {s}")),
    }
}

/// The shader heap the driver will reserve for `exe` (a file name such as `xcl.exe`): `None` where
/// the exe is in no profile or its profile does not set it (then the driver's 512 MB applies).
pub fn status(exe: &str) -> Result<Option<u32>, String> {
    let api = api()?;
    let s = Session::open(&api)?;
    let name = wide(exe);
    let mut profile = std::ptr::null_mut();
    let mut app = zeroed::<Application>();
    app.version = ver::<Application>(1);
    unsafe {
        if !found("FindApplicationByName", (api.find_app)(s.h, name.as_ptr(), &mut profile, &mut *app))? {
            return Ok(None);
        }
        let mut set = zeroed::<Setting>();
        set.version = ver::<Setting>(1);
        if !found("GetSetting", (api.get_setting)(s.h, profile, SHADER_HEAP_VA, &mut *set))? {
            return Ok(None);
        }
        Ok(Some(set.current.u32))
    }
}

/// Puts `exes` into [`PROFILE`] (made if missing) with the shader heap at `bytes`, and saves.
/// Writing the driver's settings may need an elevated process.
pub fn install(exes: &[&str], bytes: u32) -> Result<(), String> {
    if bytes % 0x1_0000 != 0 {
        return Err("the shader heap must be a multiple of 64 KiB".into());
    }
    let api = api()?;
    let s = Session::open(&api)?;
    let pname = wide(PROFILE);
    let mut profile = std::ptr::null_mut();
    unsafe {
        if !found("FindProfileByName", (api.find_profile)(s.h, pname.as_ptr(), &mut profile))? {
            let mut p = zeroed::<Profile>();
            p.version = ver::<Profile>(1);
            p.name = pname;
            check("CreateProfile", (api.create_profile)(s.h, &mut *p, &mut profile))?;
        }
        for exe in exes {
            let mut app_profile = std::ptr::null_mut();
            let mut a = zeroed::<Application>();
            a.version = ver::<Application>(1);
            let name = wide(exe);
            if found("FindApplicationByName", (api.find_app)(s.h, name.as_ptr(), &mut app_profile, &mut *a))?
                && app_profile == profile
            {
                continue;
            }
            let mut a = zeroed::<Application>();
            a.version = ver::<Application>(1);
            a.app_name = name;
            a.friendly_name = wide(exe);
            check("CreateApplication", (api.create_app)(s.h, profile, &mut *a))?;
        }
        let mut set = zeroed::<Setting>();
        set.version = ver::<Setting>(1);
        set.id = SHADER_HEAP_VA;
        set.kind = 0; // NVDRS_DWORD_TYPE
        set.current.u32 = bytes;
        let r = (api.set_setting)(s.h, profile, &mut *set);
        if r == INVALID_USER_PRIVILEGE {
            return Err("writing the driver's settings needs an elevated prompt (run as administrator)".into());
        }
        check("SetSetting", r)?;
        check("SaveSettings", (api.save)(s.h))
    }
}

/// Deletes [`PROFILE`], if there is one.
pub fn remove() -> Result<bool, String> {
    let api = api()?;
    let s = Session::open(&api)?;
    let pname = wide(PROFILE);
    let mut profile = std::ptr::null_mut();
    unsafe {
        if !found("FindProfileByName", (api.find_profile)(s.h, pname.as_ptr(), &mut profile))? {
            return Ok(false);
        }
        check("DeleteProfile", (api.delete_profile)(s.h, profile))?;
        check("SaveSettings", (api.save)(s.h))?;
    }
    Ok(true)
}

const END_ENUMERATION: Status = -7;

// The read path: its own struct and loader, so a read command reaches no write function and an
// older driver lacking one of these ids does not break `status` / `install` / `remove`.
const ID_CREATE_SESSION: u32 = 0x0694_D52E;
const ID_DESTROY_SESSION: u32 = 0xDAD9_CFF8;
const ID_LOAD: u32 = 0x375D_BD6B;
const ID_FIND_APP: u32 = 0xEEE5_66B2;
const ID_FIND_PROFILE: u32 = 0x7E4A_9A0B;
const ID_GET_SETTING: u32 = 0x73BF_8338;
const ID_ENUM_PROFILES: u32 = 0xBC37_1EE0;
const ID_GET_NUM_PROFILES: u32 = 0x1DAE_4FBC;
const ID_GET_PROFILE_INFO: u32 = 0x61CD_6FD6;
const ID_ENUM_APPS: u32 = 0x7FA2_173A;
const ID_ENUM_SETTINGS: u32 = 0xAE30_39DA;
const ID_SETTING_NAME: u32 = 0xD61C_BE6E;
const ID_BASE_PROFILE: u32 = 0xDA84_66A0;
const ID_GLOBAL_PROFILE: u32 = 0x617B_FF9F;
/// Every id [`read_api`] loads (besides `NvAPI_Initialize`).
const READ_IDS: [u32; 14] = [
    ID_CREATE_SESSION, ID_DESTROY_SESSION, ID_LOAD, ID_FIND_APP, ID_FIND_PROFILE, ID_GET_SETTING, ID_ENUM_PROFILES,
    ID_GET_NUM_PROFILES, ID_GET_PROFILE_INFO, ID_ENUM_APPS, ID_ENUM_SETTINGS, ID_SETTING_NAME, ID_BASE_PROFILE,
    ID_GLOBAL_PROFILE,
];

struct ReadApi {
    create_session: unsafe extern "C" fn(*mut Handle) -> Status,
    destroy_session: unsafe extern "C" fn(Handle) -> Status,
    load: unsafe extern "C" fn(Handle) -> Status,
    find_app: unsafe extern "C" fn(Handle, *const u16, *mut Handle, *mut Application) -> Status,
    find_profile: unsafe extern "C" fn(Handle, *const u16, *mut Handle) -> Status,
    get_setting: unsafe extern "C" fn(Handle, Handle, u32, *mut Setting) -> Status,
    enum_profiles: unsafe extern "C" fn(Handle, u32, *mut Handle) -> Status,
    num_profiles: unsafe extern "C" fn(Handle, *mut u32) -> Status,
    profile_info: unsafe extern "C" fn(Handle, Handle, *mut Profile) -> Status,
    enum_apps: unsafe extern "C" fn(Handle, Handle, u32, *mut u32, *mut Application) -> Status,
    enum_settings: unsafe extern "C" fn(Handle, Handle, u32, *mut u32, *mut Setting) -> Status,
    setting_name: unsafe extern "C" fn(u32, *mut u16) -> Status,
    base_profile: unsafe extern "C" fn(Handle, *mut Handle) -> Status,
    global_profile: unsafe extern "C" fn(Handle, *mut Handle) -> Status,
}

fn read_api() -> Result<ReadApi, String> {
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
    unsafe {
        // System32 only, as in `api()`
        let lib = LoadLibraryExW(windows::core::w!("nvapi64.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32).map_err(|e| format!("no nvapi64.dll in System32: {e}"))?;
        let qi = GetProcAddress(lib, windows::core::s!("nvapi_QueryInterface")).ok_or("no nvapi_QueryInterface")?;
        let qi: unsafe extern "C" fn(u32) -> *const c_void = std::mem::transmute(qi);
        let get = |id: u32| {
            let p = qi(id);
            if p.is_null() { Err(format!("NVAPI has no function {id:#010x}")) } else { Ok(p) }
        };
        let init: unsafe extern "C" fn() -> Status = std::mem::transmute(get(0x0150_E828)?);
        let s = init();
        if s != OK {
            return Err(format!("NvAPI_Initialize: {s}"));
        }
        Ok(ReadApi {
            create_session: std::mem::transmute(get(ID_CREATE_SESSION)?),
            destroy_session: std::mem::transmute(get(ID_DESTROY_SESSION)?),
            load: std::mem::transmute(get(ID_LOAD)?),
            find_app: std::mem::transmute(get(ID_FIND_APP)?),
            find_profile: std::mem::transmute(get(ID_FIND_PROFILE)?),
            get_setting: std::mem::transmute(get(ID_GET_SETTING)?),
            enum_profiles: std::mem::transmute(get(ID_ENUM_PROFILES)?),
            num_profiles: std::mem::transmute(get(ID_GET_NUM_PROFILES)?),
            profile_info: std::mem::transmute(get(ID_GET_PROFILE_INFO)?),
            enum_apps: std::mem::transmute(get(ID_ENUM_APPS)?),
            enum_settings: std::mem::transmute(get(ID_ENUM_SETTINGS)?),
            setting_name: std::mem::transmute(get(ID_SETTING_NAME)?),
            base_profile: std::mem::transmute(get(ID_BASE_PROFILE)?),
            global_profile: std::mem::transmute(get(ID_GLOBAL_PROFILE)?),
        })
    }
}

struct ReadSession<'a> {
    api: &'a ReadApi,
    h: Handle,
}

impl<'a> ReadSession<'a> {
    fn open(api: &'a ReadApi) -> Result<Self, String> {
        let mut h = std::ptr::null_mut();
        unsafe {
            check("CreateSession", (api.create_session)(&mut h))?;
            let s = ReadSession { api, h };
            check("LoadSettings", (api.load)(h))?;
            Ok(s)
        }
    }
}

impl Drop for ReadSession<'_> {
    fn drop(&mut self) {
        unsafe { (self.api.destroy_session)(self.h) };
    }
}

/// The text of a wide string up to its NUL.
fn wide_text(w: &[u16]) -> String {
    let end = w.iter().position(|&c| c == 0).unwrap_or(w.len());
    String::from_utf16_lossy(&w[..end])
}

fn profiles(api: &ReadApi, s: Handle) -> Result<Vec<Handle>, String> {
    let mut out = Vec::new();
    for index in 0.. {
        let mut h = std::ptr::null_mut();
        match unsafe { (api.enum_profiles)(s, index, &mut h) } {
            OK => out.push(h),
            END_ENUMERATION => break,
            r => return Err(format!("NvAPI_DRS_EnumProfiles: {r}")),
        }
    }
    Ok(out)
}

fn profile_info(api: &ReadApi, s: Handle, h: Handle) -> Result<Box<Profile>, String> {
    let mut p = zeroed::<Profile>();
    p.version = ver::<Profile>(1);
    check("GetProfileInfo", unsafe { (api.profile_info)(s, h, &mut *p) })?;
    Ok(p)
}

/// The names of the (up to) `n` applications of a profile.
fn apps(api: &ReadApi, s: Handle, h: Handle, n: u32) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    while (out.len() as u32) < n {
        let mut count = n - out.len() as u32;
        let mut buf: Vec<Application> = (0..count).map(|_| *zeroed::<Application>()).collect();
        for a in buf.iter_mut() {
            a.version = ver::<Application>(1);
        }
        match unsafe { (api.enum_apps)(s, h, out.len() as u32, &mut count, buf.as_mut_ptr()) } {
            OK => {}
            END_ENUMERATION => break,
            r => return Err(format!("NvAPI_DRS_EnumApplications: {r}")),
        }
        if count == 0 {
            break;
        }
        out.extend(buf.iter().take(count as usize).map(|a| wide_text(&a.app_name)));
    }
    Ok(out)
}

/// The (up to) `n` settings stored in a profile (each element is 12 KB).
fn settings(api: &ReadApi, s: Handle, h: Handle, n: u32) -> Result<Vec<Setting>, String> {
    let mut out: Vec<Setting> = Vec::new();
    while (out.len() as u32) < n {
        let mut count = n - out.len() as u32;
        let mut buf: Vec<Setting> = (0..count).map(|_| *zeroed::<Setting>()).collect();
        for x in buf.iter_mut() {
            x.version = ver::<Setting>(1);
        }
        match unsafe { (api.enum_settings)(s, h, out.len() as u32, &mut count, buf.as_mut_ptr()) } {
            OK => {}
            END_ENUMERATION => break,
            r => return Err(format!("NvAPI_DRS_EnumSettings: {r}")),
        }
        if count == 0 {
            break;
        }
        buf.truncate(count as usize);
        out.append(&mut buf);
    }
    Ok(out)
}

/// The setting's name: the structure's own, else the driver's, else `(unnamed)`.
fn setting_name(api: &ReadApi, id: u32, from_struct: &Wide) -> String {
    if from_struct[0] != 0 {
        return wide_text(from_struct);
    }
    let mut w = zeroed::<Wide>();
    if unsafe { (api.setting_name)(id, w.as_mut_ptr()) } == OK && w[0] != 0 {
        return wide_text(&*w);
    }
    "(unnamed)".into()
}

fn format_value(kind: u32, v: &Value) -> String {
    // SAFETY: every member of the union is plain integers, valid for any bit pattern.
    unsafe {
        match kind {
            0 => format!("{:#010X} ({})", v.u32, v.u32),
            1 => {
                let len = v.binary.len;
                let hex: String = v.binary.data.iter().take(len.min(32) as usize).map(|b| format!("{b:02x}")).collect();
                format!("len {len}: {hex}")
            }
            2 | 3 => wide_text(&v.wide),
            4 => {
                let q = v.binary.len as u64 | (u32::from_le_bytes(v.binary.data[0..4].try_into().unwrap()) as u64) << 32;
                format!("{q:#018X}")
            }
            5 => format!("{}", f32::from_bits(v.u32)),
            k => format!("type {k}"),
        }
    }
}

fn location_name(loc: u32) -> &'static str {
    match loc {
        0 => "this profile",
        1 => "global profile",
        2 => "base profile",
        3 => "driver default",
        _ => "unknown location",
    }
}

/// Case-insensitive substring.
fn matches(pattern: &str, text: &str) -> bool {
    text.to_lowercase().contains(&pattern.to_lowercase())
}

/// The settings this work looks at (shader heap, shader cache, power, Resizable BAR, ...).
const CANDIDATES: [u32; 13] = [
    0x001A_BC0F, 0x0019_8FFF, 0x00AC_8497, 0x1057_EB71, 0x1083_5005, 0x1083_5016, 0x1083_5017, 0x1083_5002,
    0x00A8_79CF, 0x007B_A09E, 0x000B_FA21, 0x000F_00BA, 0x000F_00FF,
];

fn kind_name(kind: u32) -> &'static str {
    match kind {
        0 => "dword",
        1 => "binary",
        2 => "string",
        3 => "wstring",
        4 => "qword",
        5 => "float",
        _ => "unknown",
    }
}

/// Prints what the driver stores for `target` (an exe name such as `xcl.exe`, else a profile name). Reads only.
pub fn dump(target: &str) -> Result<(), String> {
    let api = read_api()?;
    let s = ReadSession::open(&api)?;
    let name = wide(target);
    let mut profile = std::ptr::null_mut();
    let mut app = zeroed::<Application>();
    app.version = ver::<Application>(1);
    let by_app = unsafe { found("FindApplicationByName", (api.find_app)(s.h, name.as_ptr(), &mut profile, &mut *app))? };
    if !by_app && !unsafe { found("FindProfileByName", (api.find_profile)(s.h, name.as_ptr(), &mut profile))? } {
        return Err(format!("no profile or executable named {target:?}; `xcl nvprofile list <pattern>` searches"));
    }
    let info = profile_info(&api, s.h, profile)?;
    println!("profile: {}  (predefined: {})", wide_text(&info.name), if info.is_predefined != 0 { "yes" } else { "no" });
    let names = apps(&api, s.h, profile, info.num_apps)?;
    println!("applications ({}): {}", info.num_apps, names.join(", "));
    println!("settings ({}):", info.num_settings);
    for x in settings(&api, s.h, profile, info.num_settings)? {
        let predefined = if x.is_predefined_valid != 0 { format_value(x.kind, &x.predefined) } else { "-".into() };
        println!(
            "0x{:08X}  {}  {}  current={}  predefined={}  {}  [{}]",
            x.id,
            setting_name(&api, x.id, &x.name),
            kind_name(x.kind),
            format_value(x.kind, &x.current),
            predefined,
            if x.is_current_predefined == 0 { "user-set" } else { "predefined" },
            location_name(x.location),
        );
    }
    println!("resolved for this profile:");
    for id in CANDIDATES {
        let mut set = zeroed::<Setting>();
        set.version = ver::<Setting>(1);
        if unsafe { found("GetSetting", (api.get_setting)(s.h, profile, id, &mut *set))? } {
            println!("0x{id:08X}  {}  {}  [{}]", setting_name(&api, id, &set.name), format_value(set.kind, &set.current), location_name(set.location));
        } else {
            println!("0x{id:08X}  driver default (not stored)");
        }
    }
    println!("values of internal settings may be masked by the driver");
    Ok(())
}

/// Lists the profiles whose name or application matches `pattern`; with `setting`, only those that set it themselves.
pub fn list(pattern: Option<&str>, setting: Option<u32>) -> Result<(), String> {
    let api = read_api()?;
    let s = ReadSession::open(&api)?;
    let mut n = 0u32;
    check("GetNumProfiles", unsafe { (api.num_profiles)(s.h, &mut n) })?;
    for (label, f) in [("base profile", api.base_profile), ("current global profile", api.global_profile)] {
        let mut h = std::ptr::null_mut();
        check("GetBaseProfile / GetCurrentGlobalProfile", unsafe { f(s.h, &mut h) })?;
        println!("{label}: {}", wide_text(&profile_info(&api, s.h, h)?.name));
    }
    println!("{n} profiles");
    for h in profiles(&api, s.h)? {
        let info = profile_info(&api, s.h, h)?;
        let pname = wide_text(&info.name);
        let names = apps(&api, s.h, h, info.num_apps)?;
        if let Some(p) = pattern {
            if !matches(p, &pname) && !names.iter().any(|a| matches(p, a)) {
                continue;
            }
        }
        let mut line = format!("{pname} | predefined: {} | apps: {} | settings: {}", info.is_predefined != 0, names.join(", "), info.num_settings);
        if let Some(id) = setting {
            let mut set = zeroed::<Setting>();
            set.version = ver::<Setting>(1);
            if !unsafe { found("GetSetting", (api.get_setting)(s.h, h, id, &mut *set))? } || set.location != 0 {
                continue;
            }
            line += &format!(" | 0x{id:08X} = {}", format_value(set.kind, &set.current));
        }
        println!("{line}");
    }
    Ok(())
}

/// What `xcl nvprofile` is asked to do.
#[derive(Debug, PartialEq)]
enum Command {
    Status,
    Install(u32),
    Remove,
    Dump(String),
    List { pattern: Option<String>, setting: Option<u32> },
}

fn parse(args: &[String]) -> Result<Command, String> {
    let mut p = crate::cli::Parser::new(args);
    let cmd = match p.next_arg() {
        None | Some("status") => Command::Status,
        Some("remove") => Command::Remove,
        Some("dump") => Command::Dump(p.value("dump NAME")?.to_string()),
        Some("list") => {
            let (mut pattern, mut setting) = (None, None);
            while let Some(a) = p.next_arg() {
                match a {
                    "--setting" => {
                        let v = p.value("--setting")?;
                        let id = crate::cli::parse_hex_u64_str(v).map_err(|e| format!("--setting: {e}"))?;
                        setting = Some(u32::try_from(id).map_err(|_| format!("--setting {v}: does not fit 32 bits"))?);
                    }
                    _ if a.starts_with('-') || pattern.is_some() => return Err(crate::cli::Parser::unexpected(a)),
                    _ => pattern = Some(a.to_string()),
                }
            }
            return Ok(Command::List { pattern, setting });
        }
        Some("install") => {
            let mut mb = 1024;
            while let Some(a) = p.next_arg() {
                match a {
                    "--mb" => mb = p.parse_usize("--mb")?,
                    _ => return Err(crate::cli::Parser::unexpected(a)),
                }
            }
            // 4096 MB is 2^32 bytes, which does not fit in the setting's u32.
            if !(512..=4095).contains(&mb) {
                return Err(format!("--mb {mb}: expected 512 - 4095"));
            }
            return Ok(Command::Install((mb as u32) << 20));
        }
        Some(a) => return Err(crate::cli::Parser::unexpected(a)),
    };
    match p.next_arg() {
        Some(a) => Err(crate::cli::Parser::unexpected(a)),
        None => Ok(cmd),
    }
}

/// `xcl nvprofile [status | install [--mb N] | remove | dump NAME | list [PATTERN] [--setting ID]]`.
pub fn run(args: &[String]) -> i32 {
    let cmd = match parse(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("xcl nvprofile: {e}");
            return 2;
        }
    };
    let read = match &cmd {
        Command::Dump(target) => Some(dump(target)),
        Command::List { pattern, setting } => Some(list(pattern.as_deref(), *setting)),
        _ => None,
    };
    if let Some(r) = read {
        return match r {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("xcl nvprofile: {e}");
                1
            }
        };
    }
    let done = match cmd {
        Command::Dump(_) | Command::List { .. } => Ok(()),
        Command::Status => Ok(()),
        Command::Install(bytes) => install(&["xcl.exe"], bytes).map(|()| println!("xcl.exe is in the profile \"{PROFILE}\"")),
        Command::Remove => remove().map(|had| println!("{}", if had { "profile removed" } else { "no XCL profile" })),
    };
    let shown = done.and_then(|()| status("xcl.exe")).map(|v| match v {
        Some(b) => println!("xcl.exe: shader heap {} MB", b >> 20),
        None => println!("xcl.exe: the driver's default shader heap (512 MB on D3D12)"),
    });
    match shown {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("xcl nvprofile: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_sizes_match_nvapi() {
        assert_eq!(std::mem::size_of::<Profile>(), 4 + 4096 + 16);
        assert_eq!(std::mem::size_of::<Application>(), 8 + 3 * 4096);
        assert_eq!(std::mem::size_of::<Setting>(), 4 + 4096 + 20 + 2 * 4100);
    }

    #[test]
    fn the_command_line_is_checked() {
        let a = |s: &[&str]| parse(&s.iter().map(|x| x.to_string()).collect::<Vec<_>>());
        assert_eq!(a(&[]), Ok(Command::Status));
        assert_eq!(a(&["remove"]), Ok(Command::Remove));
        assert_eq!(a(&["install"]), Ok(Command::Install(1 << 30)));
        assert_eq!(a(&["install", "--mb", "2048"]), Ok(Command::Install(1 << 31)));
        assert_eq!(a(&["install", "--mb", "4095"]), Ok(Command::Install(4095 << 20)));
        assert!(a(&["install", "--mb", "4096"]).is_err());
        assert!(a(&["install", "--mb", "100"]).is_err());
        assert!(a(&["install", "--mb"]).is_err());
        assert!(a(&["status", "x"]).is_err());
        assert!(a(&["frobnicate"]).is_err());
        assert_eq!(a(&["dump", "xcl.exe"]), Ok(Command::Dump("xcl.exe".into())));
        assert!(a(&["dump"]).is_err());
        assert!(a(&["dump", "a", "b"]).is_err());
        assert_eq!(a(&["list"]), Ok(Command::List { pattern: None, setting: None }));
        assert_eq!(
            a(&["list", "rdr", "--setting", "0x001abc0f"]),
            Ok(Command::List { pattern: Some("rdr".into()), setting: Some(0x001A_BC0F) })
        );
        assert_eq!(a(&["list", "--setting", "1ABC0F"]), Ok(Command::List { pattern: None, setting: Some(0x001A_BC0F) }));
        assert!(a(&["list", "--setting", "zz"]).is_err());
        assert!(a(&["list", "--setting"]).is_err());
        assert!(a(&["list", "a", "b"]).is_err());
    }

    #[test]
    fn values_are_formatted_by_type() {
        let mut data = [0u8; 4096];
        data[..4].copy_from_slice(&1u32.to_le_bytes());
        // a QWORD's low dword is where `len` sits, its high dword the first four data bytes
        let q = Value { binary: Binary { len: 0, data } };
        assert_eq!(format_value(4, &q), "0x0000000100000000");
        assert_eq!(format_value(0, &Value { u32: 0x8000_0000 }), "0x80000000 (2147483648)");
        let mut w = wide("hi");
        w[3] = b'x' as u16; // past the NUL: not read
        assert_eq!(format_value(3, &Value { wide: w }), "hi");
        let mut bytes = [0u8; 4096];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = i as u8;
        }
        assert_eq!(format_value(1, &Value { binary: Binary { len: 2, data: bytes } }), "len 2: 0001");
        let long = format_value(1, &Value { binary: Binary { len: 100, data: bytes } });
        assert_eq!(long.len(), "len 100: ".len() + 64);
        assert_eq!(format_value(5, &Value { u32: 1.5f32.to_bits() }), "1.5");
        assert_eq!(format_value(9, &Value { u32: 0 }), "type 9");
    }

    #[test]
    fn locations_are_named() {
        assert_eq!(location_name(0), "this profile");
        assert_eq!(location_name(1), "global profile");
        assert_eq!(location_name(2), "base profile");
        assert_eq!(location_name(3), "driver default");
        assert_eq!(location_name(4), "unknown location");
    }

    #[test]
    fn pattern_matching_ignores_case() {
        assert!(matches("XCL", "My xcl.exe"));
        assert!(matches("", "anything"));
        assert!(!matches("test.exe", "other.exe"));
    }

    #[test]
    fn the_read_path_loads_no_write_function() {
        for id in [0x577D_D202u32, 0xFCBC_7E14, 0xCC17_6068, 0x4347_A9DE, 0x1709_3206] {
            assert!(!READ_IDS.contains(&id), "{id:#010x} writes");
        }
    }

    #[test]
    fn only_not_found_reads_as_absent() {
        assert_eq!(found("x", OK), Ok(true));
        assert_eq!(found("x", PROFILE_NOT_FOUND), Ok(false));
        assert_eq!(found("x", EXECUTABLE_NOT_FOUND), Ok(false));
        assert_eq!(found("x", SETTING_NOT_FOUND), Ok(false));
        assert!(found("x", -1).is_err()); // NVAPI_ERROR
        assert!(found("x", -182).is_err()); // NVAPI_EXECUTABLE_PATH_IS_AMBIGUOUS
    }
}
