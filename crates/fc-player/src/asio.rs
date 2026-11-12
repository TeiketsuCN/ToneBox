//! ASIO 输出后端（Windows）。
//!
//! 为什么自己写 COM 调用：cpal 的 ASIO 支持要用 asio-sys，而 asio-sys 在构建时用
//! bindgen 解析 ASIO 头文件——它要求 libclang 和一套跟本机 MinGW 头文件兼容的 clang。
//! 为了一个后端去装 LLVM 不划算，而且 ASIO 的接口面很小（24 个虚函数 + 几个结构体），
//! 手写一遍反而干净，构建时也不再依赖 ASIO SDK。
//!
//! 约定和独占输出一致：**32-bit 整数 / 2ch / 源采样率**，ffmpeg 按 s32le 解码，
//! 队列里的字节原样交给驱动（直通），不做音量、不做 ReplayGain。
//!
//! 驱动发现：注册表 `HKLM\SOFTWARE\ASIO\<名字>` 里的 `CLSID`；
//! 打开方式：`CoCreateInstance(clsid, 0, CLSCTX_INPROC_SERVER, clsid, ...)`
//! ——ASIO 驱动的 riid 就是它自己的 CLSID（SDK 里 asiolist.cpp 就是这么写的）。

#![allow(non_snake_case, dead_code)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, Result};

use crate::Inner;

// ── ASIO / Win32 结构 ──────────────────────────────────
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq)]
pub struct Guid {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct AsioSamples {
    hi: u32,
    lo: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct AsioTimeStamp {
    hi: u32,
    lo: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AsioBufferInfo {
    pub is_input: i32,
    pub channel_num: i32,
    pub buffers: [*mut c_void; 2],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct AsioChannelInfo {
    pub channel: i32,
    pub is_input: i32,
    pub is_active: i32,
    pub channel_group: i32,
    /// ASIOSampleType
    pub ty: i32,
    pub name: [u8; 32],
}

/// ASIOTime：我们不用它，但回调签名要一个占位（够大就行）
#[repr(C)]
#[derive(Clone, Copy)]
pub struct AsioTime {
    _buf: [u8; 64],
}

#[repr(C)]
pub struct AsioCallbacks {
    pub buffer_switch: Option<unsafe extern "system" fn(i32, i32)>,
    pub sample_rate_did_change: Option<unsafe extern "system" fn(f64)>,
    pub asio_message: Option<unsafe extern "system" fn(i32, i32, *mut c_void, *mut f64) -> i32>,
    pub buffer_switch_time_info:
        Option<unsafe extern "system" fn(*mut AsioTime, i32, i32) -> *mut AsioTime>,
}

/// IASIO 虚表：IUnknown 3 个 + ASIO 21 个，顺序严格按 iasiodrv.h
#[repr(C)]
struct AsioVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    init: unsafe extern "system" fn(*mut c_void, *mut c_void) -> i32,
    get_driver_name: unsafe extern "system" fn(*mut c_void, *mut i8),
    get_driver_version: unsafe extern "system" fn(*mut c_void) -> i32,
    get_error_message: unsafe extern "system" fn(*mut c_void, *mut i8),
    start: unsafe extern "system" fn(*mut c_void) -> i32,
    stop: unsafe extern "system" fn(*mut c_void) -> i32,
    get_channels: unsafe extern "system" fn(*mut c_void, *mut i32, *mut i32) -> i32,
    get_latencies: unsafe extern "system" fn(*mut c_void, *mut i32, *mut i32) -> i32,
    get_buffer_size:
        unsafe extern "system" fn(*mut c_void, *mut i32, *mut i32, *mut i32, *mut i32) -> i32,
    can_sample_rate: unsafe extern "system" fn(*mut c_void, f64) -> i32,
    get_sample_rate: unsafe extern "system" fn(*mut c_void, *mut f64) -> i32,
    set_sample_rate: unsafe extern "system" fn(*mut c_void, f64) -> i32,
    get_clock_sources: unsafe extern "system" fn(*mut c_void, *mut c_void, *mut i32) -> i32,
    set_clock_source: unsafe extern "system" fn(*mut c_void, i32) -> i32,
    get_sample_position:
        unsafe extern "system" fn(*mut c_void, *mut AsioSamples, *mut AsioTimeStamp) -> i32,
    get_channel_info: unsafe extern "system" fn(*mut c_void, *mut AsioChannelInfo) -> i32,
    create_buffers:
        unsafe extern "system" fn(*mut c_void, *mut AsioBufferInfo, i32, i32, *mut AsioCallbacks) -> i32,
    dispose_buffers: unsafe extern "system" fn(*mut c_void) -> i32,
    control_panel: unsafe extern "system" fn(*mut c_void) -> i32,
    future: unsafe extern "system" fn(*mut c_void, i32, *mut c_void) -> i32,
    output_ready: unsafe extern "system" fn(*mut c_void) -> i32,
}

#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(pv: *mut c_void, coinit: u32) -> i32;
    fn CoUninitialize();
    fn CoCreateInstance(
        clsid: *const Guid,
        unknown: *mut c_void,
        ctx: u32,
        iid: *const Guid,
        out: *mut *mut c_void,
    ) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn GetDesktopWindow() -> *mut c_void;
}

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(path: *const u16) -> isize;
    fn GetProcAddress(lib: isize, name: *const u8) -> *mut c_void;
    fn FreeLibrary(lib: isize) -> i32;
}

/// IClassFactory：ASIO 驱动 DLL 没在 COM 注册时，直接走 DllGetClassObject
#[repr(C)]
struct ClassFactoryVtbl {
    query_interface: unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
    add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
    release: unsafe extern "system" fn(*mut c_void) -> u32,
    create_instance:
        unsafe extern "system" fn(*mut c_void, *mut c_void, *const Guid, *mut *mut c_void) -> i32,
    lock_server: unsafe extern "system" fn(*mut c_void, i32) -> i32,
}

#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(key: isize, sub: *const u16, opts: u32, sam: u32, out: *mut isize) -> i32;
    fn RegEnumKeyExW(
        key: isize,
        index: u32,
        name: *mut u16,
        name_len: *mut u32,
        reserved: *mut c_void,
        class: *mut u16,
        class_len: *mut u32,
        last_write: *mut c_void,
    ) -> i32;
    fn RegQueryValueExW(
        key: isize,
        value: *const u16,
        reserved: *mut c_void,
        ty: *mut u32,
        data: *mut u8,
        len: *mut u32,
    ) -> i32;
    fn RegCloseKey(key: isize) -> i32;
}

const HKEY_LOCAL_MACHINE: isize = 0x8000_0002u32 as isize;
const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as isize;
const KEY_READ: u32 = 0x2_0019;
const KEY_ENUMERATE_SUB_KEYS: u32 = 0x8;
const ERROR_SUCCESS: i32 = 0;
const ERROR_NO_MORE_ITEMS: i32 = 259;
const CLSCTX_INPROC_SERVER: u32 = 0x1;
const COINIT_MULTITHREADED: u32 = 0x0;
const COINIT_APARTMENTTHREADED: u32 = 0x2;
const ASIO_OK: i32 = 0;
const ASIOST_INT32_LSB: i32 = 11;
const S_OK: i32 = 0;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn utf16z(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

fn reg_string(buf: &[u8], len: u32) -> String {
    let n = (len as usize / 2).min(buf.len() / 2);
    let units: Vec<u16> = (0..n)
        .map(|i| u16::from_le_bytes([buf[i * 2], buf[i * 2 + 1]]))
        .collect();
    utf16z(&units)
}

/// 创建一个 ASIO 驱动对象。
///
/// 正常路径是 CoCreateInstance；但有些驱动（比如本机这个 Realtek ASIO）只往
/// `HKLM\SOFTWARE\ASIO` 写了 CLSID，**没有在 COM 里注册类**，CoCreateInstance
/// 会返回 REGDB_E_CLASSNOTREG。这时退一步：直接 LoadLibrary 驱动 DLL，
/// 调它的 DllGetClassObject 拿 IClassFactory 再 CreateInstance——不用改注册表，
/// 也不需要管理员权限。
unsafe fn create_object(clsid: Guid) -> Result<(*mut c_void, isize)> {
    let mut obj: *mut c_void = std::ptr::null_mut();
    let rc = CoCreateInstance(
        &clsid,
        std::ptr::null_mut(),
        CLSCTX_INPROC_SERVER,
        &clsid,
        &mut obj,
    );
    if rc == S_OK && !obj.is_null() {
        return Ok((obj, 0));
    }

    // 兜底：System32 / SysWOW64 里名字带 asio 的 DLL 挨个试
    let mut dirs = Vec::new();
    if let Ok(w) = std::env::var("SystemRoot") {
        dirs.push(format!("{w}\\System32"));
        dirs.push(format!("{w}\\SysWOW64"));
    }
    let mut dlls: Vec<std::path::PathBuf> = Vec::new();
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                let name = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
                if name.ends_with(".dll") && name.contains("asio") {
                    dlls.push(p);
                }
            }
        }
    }
    for dll in dlls {
        let lib = LoadLibraryW(wide(&dll.to_string_lossy()).as_ptr());
        if lib == 0 {
            continue;
        }
        let fp = GetProcAddress(lib, b"DllGetClassObject\0".as_ptr());
        if fp.is_null() {
            FreeLibrary(lib);
            continue;
        }
        let get_class: unsafe extern "system" fn(*const Guid, *const Guid, *mut *mut c_void) -> i32 =
            std::mem::transmute(fp);
        let iid_factory = Guid {
            d1: 1,
            d2: 0,
            d3: 0,
            d4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46],
        };
        let mut factory: *mut c_void = std::ptr::null_mut();
        if get_class(&clsid, &iid_factory, &mut factory) != S_OK || factory.is_null() {
            FreeLibrary(lib);
            continue;
        }
        let vt = *(factory as *mut *const ClassFactoryVtbl);
        let mut obj2: *mut c_void = std::ptr::null_mut();
        let hr = ((*vt).create_instance)(factory, std::ptr::null_mut(), &clsid, &mut obj2);
        ((*vt).release)(factory);
        if hr == S_OK && !obj2.is_null() {
            return Ok((obj2, lib));
        }
        FreeLibrary(lib);
    }
    Err(anyhow!(
        "ASIO 驱动创建失败（COM 0x{rc:08X}；按 DLL 直载也没成）"
    ))
}

/// 从 "{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}" 解析 GUID
pub fn parse_guid(s: &str) -> Option<Guid> {
    let t: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if t.len() != 32 {
        return None;
    }
    let hex = |from: usize, len: usize| -> Option<u32> {
        let mut v = 0u32;
        for i in 0..len {
            v = (v << 4) | u32::from_str_radix(&t[from + i..from + i + 1], 16).ok()?;
        }
        Some(v)
    };
    let mut d4 = [0u8; 8];
    for (i, slot) in d4.iter_mut().enumerate() {
        *slot = hex(16 + i * 2, 2)? as u8;
    }
    Some(Guid {
        d1: hex(0, 8)?,
        d2: hex(8, 4)? as u16,
        d3: hex(12, 4)? as u16,
        d4,
    })
}

/// 一个注册过的 ASIO 驱动
pub struct AsioDriver {
    pub name: String,
    pub description: String,
    pub clsid: Guid,
}

/// 枚举本机的 ASIO 驱动（32/64 位注册表都看一遍）
pub fn list_drivers() -> Vec<AsioDriver> {
    let mut out: Vec<AsioDriver> = Vec::new();
    unsafe {
        for (hive, root) in [
            (HKEY_CURRENT_USER, "SOFTWARE\\ASIO"),
            (HKEY_LOCAL_MACHINE, "SOFTWARE\\ASIO"),
            (HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\ASIO"),
        ] {
            let mut key: isize = 0;
            if RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                wide(root).as_ptr(),
                0,
                KEY_READ | KEY_ENUMERATE_SUB_KEYS,
                &mut key,
            ) != ERROR_SUCCESS
            {
                continue;
            }
            let mut idx = 0u32;
            loop {
                let mut name = [0u16; 256];
                let mut len = 256u32;
                let rc = RegEnumKeyExW(
                    key,
                    idx,
                    name.as_mut_ptr(),
                    &mut len,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                if rc == ERROR_NO_MORE_ITEMS || rc != ERROR_SUCCESS {
                    break;
                }
                idx += 1;
                let drv_name = utf16z(&name);
                let sub_path = format!("{root}\\{drv_name}");
                let mut sub: isize = 0;
                if RegOpenKeyExW(
                    HKEY_LOCAL_MACHINE,
                    wide(&sub_path).as_ptr(),
                    0,
                    KEY_READ,
                    &mut sub,
                ) != ERROR_SUCCESS
                {
                    continue;
                }
                let mut buf = [0u8; 512];
                let mut buflen = buf.len() as u32;
                let mut ty = 0u32;
                let ok = RegQueryValueExW(
                    sub,
                    wide("CLSID").as_ptr(),
                    std::ptr::null_mut(),
                    &mut ty,
                    buf.as_mut_ptr(),
                    &mut buflen,
                );
                let mut dbuf = [0u8; 512];
                let mut dlen = dbuf.len() as u32;
                let ok2 = RegQueryValueExW(
                    sub,
                    wide("Description").as_ptr(),
                    std::ptr::null_mut(),
                    &mut ty,
                    dbuf.as_mut_ptr(),
                    &mut dlen,
                );
                RegCloseKey(sub);
                if ok != ERROR_SUCCESS {
                    continue;
                }
                let Some(clsid) = parse_guid(&reg_string(&buf, buflen)) else {
                    continue;
                };
                let description = if ok2 == ERROR_SUCCESS {
                    reg_string(&dbuf, dlen)
                } else {
                    drv_name.clone()
                };
                if out.iter().any(|d| d.name == drv_name) {
                    continue;
                }
                out.push(AsioDriver {
                    name: drv_name,
                    description,
                    clsid,
                });
            }
            RegCloseKey(key);
        }
    }
    out
}

// ── 回调线程看得到的运行时状态 ───────────────────────────
struct Engine {
    inner: Arc<Inner>,
    lib: isize,
    infos: Vec<AsioBufferInfo>,
    buffer_size: usize,
}

static ACTIVE: AtomicPtr<Engine> = AtomicPtr::new(std::ptr::null_mut());

unsafe extern "system" fn buffer_switch(index: i32, _now: i32) {
    let p = ACTIVE.load(Ordering::Acquire);
    if p.is_null() {
        return;
    }
    let e = &*p;
    let half = (index & 1) as usize;
    let frames = e.buffer_size;
    let playing = e.inner.playing.load(Ordering::Relaxed);
    let mut q = match e.inner.buf.lock() {
        Ok(q) => q,
        Err(_) => return,
    };
    // 队列是交错 s32le 立体声：一帧 8 字节（L 4 + R 4），拆成两条 planar
    let take = |q: &mut std::collections::VecDeque<u8>| -> (i32, i32) {
        let mut b = [0u8; 8];
        for slot in b.iter_mut() {
            *slot = q.pop_front().unwrap_or(0);
        }
        (
            i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            i32::from_le_bytes([b[4], b[5], b[6], b[7]]),
        )
    };
    let avail = q.len() / 8;
    let n = if playing { frames.min(avail) } else { 0 };

    for (ch, info) in e.infos.iter().take(2).enumerate() {
        let dst = info.buffers[half] as *mut i32;
        if dst.is_null() {
            continue;
        }
        for f in 0..frames {
            let v = if f < n {
                let (l, r) = take(&mut q);
                if ch == 0 { l } else { r }
            } else {
                0
            };
            *dst.add(f) = v;
        }
    }
    if n > 0 {
        e.inner.pos_frames.fetch_add(n as u64, Ordering::Relaxed);
    }
}

unsafe extern "system" fn sample_rate_did_change(_rate: f64) {}
unsafe extern "system" fn asio_message(_s: i32, _v: i32, _m: *mut c_void, _o: *mut f64) -> i32 {
    0
}
unsafe extern "system" fn buffer_switch_time_info(
    params: *mut AsioTime,
    index: i32,
    now: i32,
) -> *mut AsioTime {
    buffer_switch(index, now);
    params
}

pub struct Out {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Out {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        // 驱动停完、线程退出后，才能把 Engine 释放掉
        let p = ACTIVE.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !p.is_null() {
            unsafe { drop(Box::from_raw(p)) };
        }
    }
}

impl Drop for Out {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 打开第一个（或指定名字的）ASIO 驱动并跑起来。
/// 返回 (后端, 实际采样率, 声道数, 一行描述)
pub fn start(
    driver_name: Option<&str>,
    source_rate: u32,
    inner: Arc<Inner>,
) -> Result<(Out, u32, u16, String)> {
    let drivers = list_drivers();
    let drv = match driver_name.filter(|s| !s.is_empty()) {
        Some(want) => drivers
            .iter()
            .find(|d| d.name == want || d.description == want)
            .or_else(|| drivers.first()),
        None => drivers.first(),
    }
    .ok_or_else(|| anyhow!("这台机器没装 ASIO 驱动"))?;

    let clsid = drv.clsid;
    let label_name = if drv.description.is_empty() {
        drv.name.clone()
    } else {
        drv.description.clone()
    };
    let (tx, rx) = channel::<Result<(u32, u16, String)>>();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();

    let handle = std::thread::spawn(move || {
        if let Err(e) = open_and_run(clsid, label_name, source_rate, inner, stop_thread, &tx) {
            let _ = tx.send(Err(e));
        }
    });

    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok((rate, ch, label))) => Ok((
            Out {
                stop,
                handle: Some(handle),
            },
            rate,
            ch,
            label,
        )),
        Ok(Err(e)) => {
            let _ = handle.join();
            Err(e)
        }
        Err(_) => {
            stop.store(true, Ordering::SeqCst);
            Err(anyhow!("ASIO 初始化超时（驱动没响应）"))
        }
    }
}

fn open_and_run(
    clsid: Guid,
    label_name: String,
    source_rate: u32,
    inner: Arc<Inner>,
    stop: Arc<AtomicBool>,
    tx: &std::sync::mpsc::Sender<Result<(u32, u16, String)>>,
) -> Result<()> {
    unsafe {
        // ASIO 驱动基本都按 STA 写（它们的控制面板要消息泵），先试 STA
        let hr = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
        let need_uninit = hr == S_OK;

        let (obj, lib) = match create_object(clsid) {
            Ok(v) => v,
            Err(e) => {
                if need_uninit {
                    CoUninitialize();
                }
                return Err(e);
            }
        };
        let vt = *(obj as *mut *const AsioVtbl);
        let vt = &*vt;

        // 有些驱动（比如 Realtek）要一个真窗口当消息泵父窗口，给桌面窗口最省事
        let hwnd = GetDesktopWindow();
        let init_rc = (vt.init)(obj, hwnd);
        if std::env::var("FC_ASIO_DEBUG").is_ok() {
            eprintln!("[asio] init rc={init_rc}");
        }
        if init_rc == 0 {
            (vt.release)(obj);
            if need_uninit {
                CoUninitialize();
            }
            return Err(anyhow!("ASIO init() 失败：驱动可能被别的程序占着"));
        }

        let mut name_buf = [0i8; 64];
        (vt.get_driver_name)(obj, name_buf.as_mut_ptr());
        let got_name = {
            let bytes: Vec<u8> = name_buf
                .iter()
                .take_while(|&&c| c != 0)
                .map(|&c| c as u8)
                .collect();
            String::from_utf8_lossy(&bytes).to_string()
        };
        let drv_name = if got_name.is_empty() {
            label_name
        } else {
            got_name
        };

        // 采样率：优先源采样率；不行退 48k / 44.1k
        let debug = std::env::var("FC_ASIO_DEBUG").is_ok();
        if debug {
            eprintln!("[asio] driver = {drv_name}");
            let (mut i0, mut o0) = (0i32, 0i32);
            let ch_rc = (vt.get_channels)(obj, &mut i0, &mut o0);
            let mut sr = 0f64;
            let sr_rc = (vt.get_sample_rate)(obj, &mut sr);
            let (mut a, mut b, mut c, mut d) = (0i32, 0i32, 0i32, 0i32);
            let bs_rc = (vt.get_buffer_size)(obj, &mut a, &mut b, &mut c, &mut d);
            eprintln!(
                "[asio] getChannels rc={ch_rc} in={i0} out={o0}; getSampleRate rc={sr_rc} = {sr}; getBufferSize rc={bs_rc} min={a} max={b} pref={c}"
            );
        }
        let mut chosen = None;
        let mut codes: Vec<(u32, i32, i32)> = Vec::new();
        for rate in [source_rate, 48_000, 44_100] {
            if rate < 8_000 {
                continue;
            }
            // 注意：Realtek ASIO 的 canSampleRate 会一直报 NotPresent，
            // 但 setSampleRate 其实能用——所以以 set 的结果为准，can 只当参考。
            let can = (vt.can_sample_rate)(obj, rate as f64);
            let set = (vt.set_sample_rate)(obj, rate as f64);
            if debug {
                eprintln!("[asio] rate {rate}: can={can} set={set}");
            }
            codes.push((rate, can, set));
            if can == ASIO_OK && set == ASIO_OK {
                chosen = Some(rate);
                break;
            }
        }
        let rate = chosen.ok_or_else(|| {
            anyhow!(
                "ASIO 驱动拒了 44.1k/48k（{}）",
                codes
                    .iter()
                    .map(|(r, c, s)| format!("{r}: can={c} set={s}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

        let (mut min_sz, mut max_sz, mut pref_sz, mut gran) = (0i32, 0i32, 0i32, 0i32);
        let bs_rc = (vt.get_buffer_size)(obj, &mut min_sz, &mut max_sz, &mut pref_sz, &mut gran);
        if debug {
            let (mut i0, mut o0) = (0i32, 0i32);
            let ch_rc = (vt.get_channels)(obj, &mut i0, &mut o0);
            let mut sr = 0f64;
            let sr_rc = (vt.get_sample_rate)(obj, &mut sr);
            eprintln!("[asio] getChannels rc={ch_rc} in={i0} out={o0}; getSampleRate rc={sr_rc} = {sr}; getBufferSize rc={bs_rc} min={min_sz} max={max_sz} pref={pref_sz}");
        }
        if bs_rc != ASIO_OK {
            (vt.release)(obj);
            if need_uninit {
                CoUninitialize();
            }
            return Err(anyhow!("ASIO getBufferSize 失败"));
        }
        let buf_size = if pref_sz > 0 { pref_sz } else { min_sz.max(256) };

        let (mut ins, mut outs) = (0i32, 0i32);
        if (vt.get_channels)(obj, &mut ins, &mut outs) != ASIO_OK || outs < 2 {
            (vt.release)(obj);
            if need_uninit {
                CoUninitialize();
            }
            return Err(anyhow!("ASIO 驱动没有立体声输出（报告 {outs} 个输出声道）"));
        }

        // 把每条输出声道都设成 32-bit 整数 LSB（和队列里的 s32le 对齐）
        for ch in 0..outs {
            let mut ci = AsioChannelInfo {
                channel: ch,
                is_input: 0,
                is_active: 0,
                channel_group: 0,
                ty: ASIOST_INT32_LSB,
                name: [0; 32],
            };
            (vt.get_channel_info)(obj, &mut ci);
        }

        let mut infos: Vec<AsioBufferInfo> = (0..outs)
            .map(|ch| AsioBufferInfo {
                is_input: 0,
                channel_num: ch,
                buffers: [std::ptr::null_mut(); 2],
            })
            .collect();
        let mut callbacks = AsioCallbacks {
            buffer_switch: Some(buffer_switch),
            sample_rate_did_change: Some(sample_rate_did_change),
            asio_message: Some(asio_message),
            buffer_switch_time_info: Some(buffer_switch_time_info),
        };
        if (vt.create_buffers)(obj, infos.as_mut_ptr(), outs, buf_size, &mut callbacks) != ASIO_OK {
            (vt.release)(obj);
            if need_uninit {
                CoUninitialize();
            }
            return Err(anyhow!("ASIO createBuffers 失败（{buf_size} 帧 / {outs} 声道）"));
        }

        let engine = Box::into_raw(Box::new(Engine {
            inner,
            lib,
            infos,
            buffer_size: buf_size as usize,
        }));
        ACTIVE.store(engine, Ordering::Release);

        if (vt.start)(obj) != ASIO_OK {
            ACTIVE.store(std::ptr::null_mut(), Ordering::Release);
            drop(Box::from_raw(engine));
            (vt.dispose_buffers)(obj);
            (vt.release)(obj);
            if need_uninit {
                CoUninitialize();
            }
            return Err(anyhow!("ASIO start() 失败"));
        }

        let label = format!("{drv_name} · ASIO · {rate} Hz · 32-bit int · 2ch（{buf_size} 帧）");
        let _ = tx.send(Ok((rate, 2, label)));

        while !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(20));
        }
        (vt.stop)(obj);
        (vt.dispose_buffers)(obj);
        (vt.release)(obj);
        if need_uninit {
            CoUninitialize();
        }
        Ok(())
    }
}

/// 探一个驱动现在能不能用（会真的打开再关掉）
pub fn probe() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for d in list_drivers() {
        let status = probe_one(d.clsid);
        out.push((d.name, status));
    }
    out
}

fn probe_one(clsid: Guid) -> String {
    unsafe {
        let hr = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
        let need_uninit = hr == S_OK;
        let (obj, lib) = match create_object(clsid) {
            Ok(v) => v,
            Err(e) => {
                if need_uninit {
                    CoUninitialize();
                }
                return format!("打不开：{e}");
            }
        };
        let vt = *(obj as *mut *const AsioVtbl);
        let vt = &*vt;
        // 有些驱动（比如 Realtek）要一个真窗口当消息泵父窗口，给桌面窗口最省事
        let hwnd = GetDesktopWindow();
        let init_rc = (vt.init)(obj, hwnd);
        if std::env::var("FC_ASIO_DEBUG").is_ok() {
            eprintln!("[asio] init rc={init_rc}");
        }
        if init_rc == 0 {
            (vt.release)(obj);
            if lib != 0 {
                FreeLibrary(lib);
            }
            if need_uninit {
                CoUninitialize();
            }
            return "init 失败（可能被别的程序占用）".to_string();
        }
        let (mut ins, mut outs) = (0i32, 0i32);
        (vt.get_channels)(obj, &mut ins, &mut outs);
        let rate = (vt.set_sample_rate)(obj, 48_000.0);
        let label = if rate == ASIO_OK {
            format!("可用 · {outs} 输出声道")
        } else {
            format!("驱动在，但报告无硬件（setSampleRate={rate}，{outs} 声道）")
        };
        (vt.release)(obj);
        if lib != 0 {
            FreeLibrary(lib);
        }
        if need_uninit {
            CoUninitialize();
        }
        label
    }
}

/// 命令行自检：打印本机认到的 ASIO 驱动和它现在的状态
pub fn describe() -> String {
    let d = list_drivers();
    if d.is_empty() {
        return "ASIO：没找到驱动（注册表 HKLM\\SOFTWARE\\ASIO 是空的）".to_string();
    }
    let status = probe();
    let mut s = format!("ASIO：{} 个驱动\n", d.len());
    for x in d {
        let st = status
            .iter()
            .find(|(n, _)| *n == x.name)
            .map(|(_, s)| s.clone())
            .unwrap_or_default();
        s.push_str(&format!("  {}  —— {}\n", x.name, st));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 注册表里的 CLSID 字符串解析（错一位就打不开驱动）
    #[test]
    fn parses_clsid() {
        let g = parse_guid("{A80362FF-CE76-4DD9-874A-704C57BF0D6A}").unwrap();
        assert_eq!(g.d1, 0xA80362FF);
        assert_eq!(g.d2, 0xCE76);
        assert_eq!(g.d3, 0x4DD9);
        assert_eq!(g.d4, [0x87, 0x4A, 0x70, 0x4C, 0x57, 0xBF, 0x0D, 0x6A]);
        assert!(parse_guid("not-a-guid").is_none());
        assert!(parse_guid("{1234}").is_none());
    }
}
/// 打开驱动的控制面板（很多驱动要先在里面选一次输出设备/时钟，才肯开流）。
/// 会弹驱动自己的窗口，调用方在 UI 线程里调。
pub fn open_control_panel() -> Result<()> {
    let Some(drv) = list_drivers().into_iter().next() else {
        return Err(anyhow!("这台机器没装 ASIO 驱动"));
    };
    unsafe {
        let hr = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
        let need_uninit = hr == S_OK;
        let (obj, lib) = create_object(drv.clsid)?;
        let vt = *(obj as *mut *const AsioVtbl);
        let vt = &*vt;
        (vt.init)(obj, std::ptr::null_mut());
        let rc = (vt.control_panel)(obj);
        (vt.release)(obj);
        if lib != 0 {
            FreeLibrary(lib);
        }
        if need_uninit {
            CoUninitialize();
        }
        if rc != ASIO_OK {
            return Err(anyhow!("控制面板打不开（{rc}）"));
        }
    }
    Ok(())
}