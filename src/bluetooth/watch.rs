use super::{
    airpods::watch_airpods_async,
    asus_mouse::watch_asus_mouse_async,
    ble::watch_ble_devices_async,
    btc::{watch_btc_devices_battery, watch_btc_devices_status_async},
    presence::watch_bt_presence_async,
};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use anyhow::Result;
use log::info;
use tokio::task::JoinHandle;

type WatchHandle = JoinHandle<Result<(), anyhow::Error>>;

macro_rules! spawn_watch {
    ($func:expr, $exit_flag:expr, $restart_flag:expr) => {{
        let exit_flag = Arc::clone(&$exit_flag);
        let restart_flag = Arc::clone(&$restart_flag);

        tokio::spawn(async move {
            let result = $func(&exit_flag, &restart_flag).await;
            if let Err(error) = &result {
                log::error!("Bluetooth watcher failed: {error:#}");
            }
            result
        })
    }};
}

pub struct Watcher {
    watch_handles: Option<[WatchHandle; 6]>,
    exit_flag: Arc<AtomicBool>,
    restart_flag: Arc<AtomicUsize>,
}

impl Watcher {
    pub fn new() -> Self {
        let exit_flag = Arc::new(AtomicBool::new(false));
        let restart_flag = Arc::new(AtomicUsize::new(0));
        Self {
            watch_handles: None,
            exit_flag,
            restart_flag,
        }
    }

    pub fn start(&mut self) {
        info!("Starting the watch bluetooth thread...");
        let watch_handles = self.watch_loop();
        self.watch_handles = Some(watch_handles);
    }

    pub fn stop(&mut self) {
        info!("Stopping the watch bluetooth thread...");
        self.exit_flag.store(true, Ordering::Relaxed);
        self.restart_flag.store(0, Ordering::Relaxed);
        self.watch_handles
            .take()
            .iter()
            .flatten()
            .for_each(|h| h.abort());
    }

    #[rustfmt::skip]
    fn watch_loop(&self) -> [WatchHandle; 6] {
        info!("The watch bluetooth thread is started.");

        let watch_ble_handle = spawn_watch!(watch_ble_devices_async, self.exit_flag, self.restart_flag);
        let watch_btc_battery_handle = spawn_watch!(watch_btc_devices_battery, self.exit_flag, self.restart_flag);
        let watch_btc_status_handle = spawn_watch!(watch_btc_devices_status_async, self.exit_flag, self.restart_flag);
        let watch_bt_presence_handle = spawn_watch!(watch_bt_presence_async, self.exit_flag, self.restart_flag);
        let watch_airpods_handle = spawn_watch!(watch_airpods_async, self.exit_flag, self.restart_flag);
        let watch_asus_mouse_handle = spawn_watch!(watch_asus_mouse_async, self.exit_flag, self.restart_flag);

        [
            watch_ble_handle,
            watch_btc_battery_handle,
            watch_btc_status_handle,
            watch_bt_presence_handle,
            watch_airpods_handle,
            watch_asus_mouse_handle,
        ]
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop();
    }
}
