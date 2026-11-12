//! 系统蓝牙设备：列出 Windows 已知的已配对/未配对设备，并调用系统配对接口。
//!
//! 说明：
//! - “已配对”列表用 `BluetoothDevice::GetDeviceSelectorFromPairingState(true)`，列表本身即已配对；
//!   `DeviceInformation.Pairing().IsPaired()` 在部分经典蓝牙设备上会误报 false，不拿它当唯一依据。
//! - 未配对列表同样是 Windows 已经发现过的设备（不是实时扫描）；新设备先在 Windows 里配对一次即可出现。
//! - PIN/确认由 Windows 的配对流程处理，这里只调用 WinRT，不碰 HCI。

use anyhow::Result;

#[derive(Clone, Debug)]
pub struct BtDevice {
    pub id: String,
    pub name: String,
    pub paired: bool,
}

#[cfg(windows)]
fn wait<T: windows::core::RuntimeType + 'static>(
    op: &windows_future::IAsyncOperation<T>,
) -> Result<T> {
    // 枚举蓝牙设备可能很慢（经典 + BLE），最多等 30 秒
    for _ in 0..3000 {
        if op.Status()? == windows_future::AsyncStatus::Completed {
            return op.GetResults().map_err(Into::into);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    anyhow::bail!("WinRT 枚举超时（30 秒）")
}

#[cfg(windows)]
pub fn list(paired_only: bool) -> Result<Vec<BtDevice>> {
    use windows::Devices::Bluetooth::BluetoothDevice;
    use windows::Devices::Enumeration::DeviceInformation;

    let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(paired_only)?;
    let coll = wait(&DeviceInformation::FindAllAsyncAqsFilter(&selector)?)?;
    let n = coll.Size()?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let d = coll.GetAt(i)?;
        let name = d.Name().map(|s| s.to_string()).unwrap_or_default();
        let id = d.Id().map(|s| s.to_string()).unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        out.push(BtDevice {
            id,
            name,
            paired: paired_only,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(windows)]
pub fn scan(seconds: u64) -> Result<Vec<BtDevice>> {
    use std::sync::mpsc;
    use windows::Devices::Bluetooth::BluetoothDevice;
    use windows::Devices::Enumeration::{DeviceInformation, DeviceWatcher};
    use windows::Foundation::TypedEventHandler;

    let selector = BluetoothDevice::GetDeviceSelectorFromPairingState(false)?;
    let watcher: DeviceWatcher = DeviceInformation::CreateWatcherAqsFilter(&selector)?;
    let (tx, rx) = mpsc::channel::<BtDevice>();
    let handler = TypedEventHandler::new(move |_: windows::core::Ref<DeviceWatcher>, info: windows::core::Ref<DeviceInformation>| {
        if let Some(d) = info.as_ref() {
            if let (Ok(name), Ok(id)) = (d.Name(), d.Id()) {
                let _ = tx.send(BtDevice {
                    id: id.to_string(),
                    name: name.to_string(),
                    paired: false,
                });
            }
        }
        Ok(())
    });
    watcher.Added(&handler)?;
    watcher.Start()?;
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    let _ = watcher.Stop();
    drop(handler);
    let mut out: Vec<BtDevice> = Vec::new();
    for d in rx.try_iter() {
        if !out.iter().any(|x| x.id == d.id) {
            out.push(d);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(windows)]
pub fn pair(id: &str) -> Result<String> {
    use windows::core::HSTRING;
    use windows::Devices::Enumeration::DeviceInformation;

    let info = wait(&DeviceInformation::CreateFromIdAsync(&HSTRING::from(id))?)?;
    let pairing = info.Pairing()?;
    let res = wait(&pairing.PairAsync()?)?;
    Ok(format!("{:?}", res.Status()?))
}

#[cfg(windows)]
pub fn unpair(id: &str) -> Result<String> {
    use windows::core::HSTRING;
    use windows::Devices::Enumeration::DeviceInformation;

    let info = wait(&DeviceInformation::CreateFromIdAsync(&HSTRING::from(id))?)?;
    let pairing = info.Pairing()?;
    let res = wait(&pairing.UnpairAsync()?)?;
    Ok(format!("{:?}", res.Status()?))
}

#[cfg(not(windows))]
pub fn list(_paired_only: bool) -> Result<Vec<BtDevice>> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub fn pair(_id: &str) -> Result<String> {
    anyhow::bail!("蓝牙配对只在 Windows 上支持")
}

#[cfg(not(windows))]
pub fn unpair(_id: &str) -> Result<String> {
    anyhow::bail!("蓝牙配对只在 Windows 上支持")
}
