//! 系统占用小采样：CPU、内存、编解码（ffmpeg 子进程）压力。
//!
//! 为什么自己调 Win32 而不用 sysinfo：这个项目到现在只有 egui/ffmpeg 这些依赖，
//! 为了三个数字再拉一个 crate 不划算，而且新依赖还要联网拉。
//!
//! 采样很便宜（几个系统调用 + 一次进程快照），但没必要每帧都做，
//! 内部按 1 秒节流。数值是"距离上次采样这段区间"的增量，所以第一次是 0。

#![allow(non_snake_case, dead_code)]

use eframe::egui;
use std::time::Instant;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct FileTime {
    low: u32,
    high: u32,
}

impl FileTime {
    fn v(self) -> u64 {
        ((self.high as u64) << 32) | self.low as u64
    }
}

#[repr(C)]
#[derive(Default)]
struct MemStatusEx {
    dw_length: u32,
    dw_memory_load: u32,
    total_phys: u64,
    avail_phys: u64,
    total_page_file: u64,
    avail_page_file: u64,
    total_virtual: u64,
    avail_virtual: u64,
    avail_extended_virtual: u64,
}

#[repr(C)]
#[derive(Default)]
struct ProcessMemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[repr(C)]
struct ProcessEntry32W {
    dw_size: u32,
    cnt_usage: u32,
    process_id: u32,
    default_heap_id: usize,
    module_id: u32,
    cnt_threads: u32,
    parent_process_id: u32,
    pri_class_base: i32,
    flags: u32,
    exe_file: [u16; 260],
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetSystemTimes(idle: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
    fn GlobalMemoryStatusEx(buf: *mut MemStatusEx) -> i32;
    fn GetCurrentProcess() -> isize;
    fn GetProcessTimes(
        h: isize,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> isize;
    fn CloseHandle(h: isize) -> i32;
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> isize;
    fn Process32FirstW(snap: isize, entry: *mut ProcessEntry32W) -> i32;
    fn Process32NextW(snap: isize, entry: *mut ProcessEntry32W) -> i32;
}

#[cfg(windows)]
#[link(name = "psapi")]
extern "system" {
    fn GetProcessMemoryInfo(h: isize, counters: *mut ProcessMemoryCounters, cb: u32) -> i32;
}

const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const INVALID_HANDLE_VALUE: isize = -1;

/// 一帧用的系统数字
#[derive(Default, Clone, Copy)]
pub(crate) struct SysStats {
    /// 整机 CPU 占用 0~1
    pub cpu_total: f32,
    /// 本程序 CPU 占用 0~1
    pub cpu_self: f32,
    /// 我们拉起来的 ffmpeg / atracdenc 子进程 CPU 占用 0~1（= 编解码压力）
    pub cpu_children: f32,
    /// 整机内存（字节）
    pub mem_total: u64,
    pub mem_avail: u64,
    /// 本程序工作集（字节）
    pub mem_self: u64,
    /// 当前活着的编解码子进程数
    pub codec_procs: u32,
}

pub(crate) struct Sampler {
    pub stats: SysStats,
    prev: Option<Sample>,
    last: Option<Instant>,
}

#[derive(Clone, Copy)]
struct Sample {
    idle: u64,
    kernel: u64,
    user: u64,
    self_cpu: u64,
    children_cpu: u64,
}

impl Sampler {
    pub fn new() -> Self {
        Self {
            stats: SysStats::default(),
            prev: None,
            last: None,
        }
    }

    /// 面板开着的时候每帧调一下；内部按 1 秒节流
    pub fn sample(&mut self, ctx: &egui::Context) {
        // 数据会过期：让界面隔一会儿自己醒一次
        ctx.request_repaint_after(std::time::Duration::from_millis(700));
        if self.last
            .map(|t| t.elapsed().as_millis() < 900)
            .unwrap_or(false)
        {
            return;
        }
        self.tick();
    }

    /// 采一次（不依赖 egui，方便单测）
    pub fn tick(&mut self) {
        let now = Instant::now();
        self.last = Some(now);
        let s = read();
        self.stats.mem_total = s.0;
        self.stats.mem_avail = s.1;
        self.stats.mem_self = s.2;
        self.stats.codec_procs = s.3;
        let cur = s.4;
        if let Some(p) = self.prev {
            let idle = cur.idle.saturating_sub(p.idle);
            let kernel = cur.kernel.saturating_sub(p.kernel);
            let user = cur.user.saturating_sub(p.user);
            let total = kernel + user; // kernel 里含 idle
            if total > 0 {
                self.stats.cpu_total = ((total.saturating_sub(idle)) as f32 / total as f32).clamp(0.0, 1.0);
                let self_d = cur.self_cpu.saturating_sub(p.self_cpu);
                let kid_d = cur.children_cpu.saturating_sub(p.children_cpu);
                self.stats.cpu_self = (self_d as f32 / total as f32).clamp(0.0, 1.0);
                self.stats.cpu_children = (kid_d as f32 / total as f32).clamp(0.0, 1.0);
            }
        }
        self.prev = Some(cur);
    }
}

/// 读一次：返回 (内存总量, 可用, 本程序占用, 编解码子进程数, CPU 时间戳)
#[cfg(windows)]
fn read() -> (u64, u64, u64, u32, Sample) {
    unsafe {
        let mut m = MemStatusEx::default();
        m.dw_length = std::mem::size_of::<MemStatusEx>() as u32;
        let ok = GlobalMemoryStatusEx(&mut m) != 0;
        let (total, avail) = if ok { (m.total_phys, m.avail_phys) } else { (0, 0) };

        let me = GetCurrentProcess();
        let mut mem_self = 0u64;
        let mut pmc = ProcessMemoryCounters::default();
        pmc.cb = std::mem::size_of::<ProcessMemoryCounters>() as u32;
        if GetProcessMemoryInfo(me, &mut pmc, pmc.cb) != 0 {
            mem_self = pmc.working_set_size as u64;
        }

        let my_pid = std::process::id();
        let self_cpu = proc_cpu(me).unwrap_or(0);

        // 子进程：快照里 th32ParentProcessID == 我们的 pid
        let mut children_cpu = 0u64;
        let mut codec_procs = 0u32;
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap != INVALID_HANDLE_VALUE && snap != 0 {
            let mut e: ProcessEntry32W = std::mem::zeroed();
            e.dw_size = std::mem::size_of::<ProcessEntry32W>() as u32;
            let mut ok = Process32FirstW(snap, &mut e) != 0;
            while ok {
                if e.parent_process_id == my_pid && e.process_id != my_pid {
                    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, e.process_id);
                    if h != 0 {
                        if let Some(c) = proc_cpu(h) {
                            children_cpu += c;
                            codec_procs += 1;
                        }
                        CloseHandle(h);
                    }
                }
                ok = Process32NextW(snap, &mut e) != 0;
            }
            CloseHandle(snap);
        }

        let (mut idle, mut kernel, mut user) =
            (FileTime::default(), FileTime::default(), FileTime::default());
        GetSystemTimes(&mut idle, &mut kernel, &mut user);

        (
            total,
            avail,
            mem_self,
            codec_procs,
            Sample {
                idle: idle.v(),
                kernel: kernel.v(),
                user: user.v(),
                self_cpu,
                children_cpu,
            },
        )
    }
}

/// 进程累计 CPU 时间（内核 + 用户，单位 100ns）
#[cfg(windows)]
fn proc_cpu(h: isize) -> Option<u64> {
    unsafe {
        let (mut c, mut e, mut k, mut u) = (
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
            FileTime::default(),
        );
        if GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) == 0 {
            return None;
        }
        Some(k.v() + u.v())
    }
}

#[cfg(not(windows))]
fn read() -> (u64, u64, u64, u32, Sample) {
    (
        0,
        0,
        0,
        0,
        Sample {
            idle: 0,
            kernel: 0,
            user: 0,
            self_cpu: 0,
            children_cpu: 0,
        },
    )
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// 枚举子进程 + 读它的 CPU 时间这条链路：拉一个会跑几秒的 cmd 子进程，
    /// 采样两次，应该能看到 1 个子进程，且它的 CPU 时间在涨。
    #[test]
    fn sees_child_process_cpu() {
        let mut child = std::process::Command::new("cmd")
            .args(["/c", "ping -n 6 127.0.0.1 > nul"])
            .spawn()
            .expect("spawn cmd");
        let mut s = Sampler::new();
        s.tick(); // 建立基线
        std::thread::sleep(std::time::Duration::from_millis(1500));
        s.tick();
        let _ = child.wait();
        assert_eq!(s.stats.codec_procs, 1, "应该看到 1 个子进程");
        assert!(
            s.stats.cpu_children > 0.0,
            crate::tr!("子进程 CPU 应该 > 0，实际 {}"),
            s.stats.cpu_children
        );
        assert!(s.stats.mem_total > 0, "整机内存应该读得到");
        assert!(s.stats.mem_self > 0, "本进程内存应该读得到");
    }
}