# BlueGauge

BlueGauge 是一个轻量的 Windows 托盘电量工具。它常驻在右下角通知区域，鼠标悬停到托盘图标上，就能看到已连接设备的剩余电量。

这个版本在原项目基础上增加了更强的设备兼容能力，重点补上了正版 AirPods Pro 和 ASUS ROG 2.4GHz 无线鼠标的电量读取。

## 当前增强

- 支持常规蓝牙设备电量显示。
- 支持 AirPods 专用解析，可读取 Apple BLE 广播里的左右耳与充电盒电量。
- 支持正版 AirPods Pro，已验证型号 `0x200E`。
- 支持 ASUS ROG Strix Impact II Wireless 2.4GHz 鼠标，设备 ID 为 `VID_0B05&PID_1949`。
- 托盘悬停提示支持多电量展示，例如 AirPods 左耳、右耳、充电盒分别显示。
- 支持低电量通知、断开/重连通知、设备别名、托盘图标样式设置。

## 已验证设备

| 设备 | 连接方式 | 显示方式 | 说明 |
| --- | --- | --- | --- |
| AirPods Pro | 蓝牙 + Apple BLE 广播 | 左耳、右耳、充电盒 | 充电盒电量需要盒子广播时才会更新 |
| ROG Strix Impact II Wireless | 2.4GHz USB 接收器 | 鼠标电量 | 该型号只上报 25% 档位 |
| 常规蓝牙耳机、键盘、鼠标 | 蓝牙 | 单一百分比 | 取决于设备是否向 Windows 暴露电量 |

## ASUS ROG 鼠标说明

2.4GHz 无线鼠标通常不是蓝牙设备，Windows 标准蓝牙电量接口读不到它们。

本版本为 `ROG Strix Impact II Wireless` 增加了 ASUS HID 专用读取逻辑，通过 USB/HID 查询包读取电量。这个型号的协议只返回粗略档位，所以托盘中会显示：

- `25%`
- `50%`
- `75%`
- `100%`

这不是显示精度问题，而是鼠标固件本身只提供这样的电量档位。

## AirPods 说明

正版 AirPods 在 Windows 上经常不会通过标准蓝牙属性暴露电量。本版本增加了 Apple AirPods 专用支持，会监听 Apple BLE 广播包并解析电量。

AirPods 显示可能包含：

- `L`：左耳电量
- `R`：右耳电量
- `C`：充电盒电量

如果只看到左右耳，没有看到充电盒，通常是因为充电盒当时没有广播电量。打开盒盖、取放耳机或重新连接后，充电盒电量更容易刷新出来。

## 使用方法

1. 运行 `BlueGauge.exe`。
2. 程序会出现在 Windows 右下角通知区域。
3. 鼠标悬停到托盘图标上，查看设备电量。
4. 右键托盘图标，可以刷新设备、选择图标样式、设置通知和打开配置文件。

发布版文件位于：

```text
target/release/BlueGauge.exe
```

## 从源码构建

需要 Rust 工具链。

```powershell
cargo build --release
```

运行测试：

```powershell
cargo test asus_mouse
```

## 技术实现

- 常规蓝牙设备：读取 Windows PnP / Bluetooth 电量属性。
- BLE 设备：读取 GATT Battery Service。
- AirPods：监听 Apple BLE manufacturer data，解析 AirPods 电量广播。
- ASUS ROG 鼠标：通过 `hidapi` 打开 ASUS HID 设备并读取专用电量包。

## 已知限制

- 不是所有蓝牙设备都会向 Windows 暴露电量。
- 不是所有 2.4GHz 无线设备都能通用读取电量，不同厂商通常使用不同私有协议。
- ASUS ROG 鼠标支持目前只针对 `ROG Strix Impact II Wireless` 做了验证。
- AirPods 充电盒电量依赖广播时机，可能不会每次都出现。
- Windows 托盘提示文本长度有限，设备太多时可能被系统截断。

## 鸣谢

本项目基于 BlueGauge 原项目改造：

- 原项目：[iKineticate/BlueGauge](https://github.com/iKineticate/BlueGauge)

协议参考：

- AirPods 广播解析参考：[SpriteOvO/AirPodsDesktop](https://github.com/SpriteOvO/AirPodsDesktop)
- ASUS ROG 鼠标协议线索参考：[seerge/g-helper](https://github.com/seerge/g-helper)

## License

MIT
