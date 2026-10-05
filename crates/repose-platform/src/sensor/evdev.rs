//! Linux motion backend reading IMU axes straight from evdev nodes.

use std::collections::HashMap;
use std::path::PathBuf;
use web_time::{Duration, Instant};

use repose_core::input::SensorSample;

use super::{SensorBackend, SensorReading};

/// Conversion for drivers that report no `AbsInfo` resolution, in the units
/// [`SensorSample`] documents: degrees per second and g per raw count.
#[derive(Clone, Copy, Debug)]
pub struct MotionScale {
    pub gyroscope: f32,
    pub accelerometer: f32,
}

impl Default for MotionScale {
    fn default() -> Self {
        Self {
            gyroscope: 0.01,
            accelerometer: 0.001,
        }
    }
}

/// Accelerometer axes, in x, y, z order.
const ACCEL_AXES: [evdev::AbsoluteAxisCode; 3] = [
    evdev::AbsoluteAxisCode::ABS_X,
    evdev::AbsoluteAxisCode::ABS_Y,
    evdev::AbsoluteAxisCode::ABS_Z,
];

/// Gyroscope axes on an accelerometer node, in x, y, z order.
const GYRO_AXES: [evdev::AbsoluteAxisCode; 3] = [
    evdev::AbsoluteAxisCode::ABS_RX,
    evdev::AbsoluteAxisCode::ABS_RY,
    evdev::AbsoluteAxisCode::ABS_RZ,
];

/// Buttons every gamepad reports, used to tell pad nodes apart from the
/// keyboards, mice and touchpads sharing the same directory.
const GAMEPAD_BUTTONS: [evdev::KeyCode; 5] = [
    evdev::KeyCode::BTN_SOUTH,
    evdev::KeyCode::BTN_EAST,
    evdev::KeyCode::BTN_NORTH,
    evdev::KeyCode::BTN_WEST,
    evdev::KeyCode::BTN_TRIGGER,
];

/// Suffixes a motion node carries that its pad node does not.
const MOTION_SUFFIXES: [&str; 2] = [" Motion Sensors", " (IMU)"];

const NINTENDO_VENDOR: u16 = 0x057e;

/// One sensor kind's three axes, plus counts-to-unit scale each.
#[derive(Clone, Copy, Debug)]
struct AxisSet {
    codes: [u16; 3],
    scale: [f32; 3],
}

impl AxisSet {
    fn index(&self, code: u16) -> Option<usize> {
        self.codes.iter().position(|candidate| *candidate == code)
    }

    fn scale_raw(&self, raw: [i32; 3]) -> [f32; 3] {
        [
            raw[0] as f32 * self.scale[0],
            raw[1] as f32 * self.scale[1],
            raw[2] as f32 * self.scale[2],
        ]
    }
}

#[derive(Debug)]
struct MotionDevice {
    /// Name the gamepad backend reports, so readings route to that pad.
    name: String,
    /// Rotates driver axes into the frame the other pads report.
    frame: fn([f32; 3]) -> [f32; 3],
    path: PathBuf,
    device: evdev::Device,
    gyroscope: Option<AxisSet>,
    accelerometer: Option<AxisSet>,
    gyro_raw: [i32; 3],
    accel_raw: [i32; 3],
    gyro_dirty: bool,
    accel_dirty: bool,
    /// The fd stopped working (unplugged, suspended). Dropped at the end of
    /// the poll, and the scan cache with it so the node is reopened.
    gone: bool,
}

impl MotionDevice {
    fn readings(&mut self) -> Vec<SensorReading> {
        let mut out = Vec::with_capacity(2);
        if self.gyro_dirty {
            let data = self
                .gyroscope
                .as_ref()
                .map(|axes| axes.scale_raw(self.gyro_raw));
            if let Some(data) = data {
                let data = (self.frame)(data);
                out.push(SensorReading {
                    device: self.name.clone(),
                    sample: SensorSample::gyroscope(data[0], data[1], data[2]),
                });
            }
            self.gyro_dirty = false;
        }
        if self.accel_dirty {
            let data = self
                .accelerometer
                .as_ref()
                .map(|axes| axes.scale_raw(self.accel_raw));
            if let Some(data) = data {
                let data = (self.frame)(data);
                out.push(SensorReading {
                    device: self.name.clone(),
                    sample: SensorSample::accelerometer(data[0], data[1], data[2]),
                });
            }
            self.accel_dirty = false;
        }
        out
    }

    fn drain(&mut self) {
        loop {
            let events = match self.read_events() {
                Ok(events) => events,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                    // The read never ran, so whatever resync the crate spent on
                    // the attempt is gone with it, and the kernel will not
                    // resend a value that repeats. Re-read instead.
                    self.resync();
                    return;
                }
                Err(error) => {
                    log::debug!("sensor: {}: {error}", self.path.display());
                    self.gone = true;
                    return;
                }
            };
            if events.is_empty() {
                // read() only reports end of file when the node is gone, but a
                // wedged fd must not be dropped on that alone.
                if self.device.get_absinfo().is_err() {
                    log::debug!("sensor: {}: node closed", self.path.display());
                    self.gone = true;
                }
                return;
            }
            for event in events {
                let evdev::EventSummary::AbsoluteAxis(_, code, value) = event.destructure() else {
                    continue;
                };
                let accel = self.accelerometer.and_then(|axes| axes.index(code.0));
                let gyro = self.gyroscope.and_then(|axes| axes.index(code.0));
                if let Some(axis) = accel {
                    self.accel_raw[axis] = value;
                    self.accel_dirty = true;
                }
                if let Some(axis) = gyro {
                    self.gyro_raw[axis] = value;
                    self.gyro_dirty = true;
                }
            }
        }
    }

    fn read_events(&mut self) -> std::io::Result<Vec<evdev::InputEvent>> {
        Ok(self.device.fetch_events()?.collect())
    }

    /// Adopt the state the node holds right now. The input core drops an
    /// absolute event that repeats the stored value, so a pad that never
    /// changes again is never heard from again.
    fn resync(&mut self) {
        let (_, values) = abs_state(&self.device);
        if values.is_empty() {
            return;
        }
        self.gyro_raw = current_raw(&values, &GYRO_AXES);
        self.accel_raw = current_raw(&values, &ACCEL_AXES);
        self.gyro_dirty = self.gyroscope.is_some();
        self.accel_dirty = self.accelerometer.is_some();
    }

    /// A zeroed gyroscope reading for a device whose fd died: its last rate
    /// would otherwise keep integrating forever. The accelerometer keeps the
    /// value it had, since a fabricated 0 g reads as free fall.
    fn stop_readings(&self) -> Vec<SensorReading> {
        if self.gyroscope.is_none() {
            return Vec::new();
        }
        vec![SensorReading {
            device: self.name.clone(),
            sample: SensorSample::gyroscope(0.0, 0.0, 0.0),
        }]
    }
}

/// Linux motion backend reading IMU axes straight from evdev nodes.
///
/// Devices are reopened only when `/dev/input` gains or loses an event node,
/// so a connected pad keeps its fds and accumulated state across scans.
pub struct EvdevSensorBackend {
    devices: Vec<MotionDevice>,
    known_paths: Vec<PathBuf>,
    scale: MotionScale,
    last_scan: Instant,
    rescan_after: Duration,
    /// A device died, so the path list alone can no longer prove the scan is
    /// current: an unplugged pad's node number can be reused inside one scan
    /// window.
    stale: bool,
}

impl EvdevSensorBackend {
    pub fn new() -> Self {
        Self::with_scale(MotionScale::default())
    }

    pub fn with_scale(scale: MotionScale) -> Self {
        let known_paths = event_paths();
        Self {
            devices: discover(&known_paths, HashMap::new(), scale),
            known_paths,
            scale,
            last_scan: Instant::now(),
            rescan_after: Duration::from_secs(2),
            stale: false,
        }
    }

    /// Names currently reporting motion, for diagnostics.
    pub fn device_names(&self) -> Vec<&str> {
        self.devices
            .iter()
            .map(|device| device.name.as_str())
            .collect()
    }

    fn rescan(&mut self) -> Vec<SensorReading> {
        self.last_scan = Instant::now();
        let known_paths = event_paths();
        if known_paths.is_empty() {
            // Unreadable directory, not "everything unplugged": keep what we
            // have, since a live fd still reports until the device really dies.
            return Vec::new();
        }
        if known_paths == self.known_paths && !self.stale {
            return Vec::new();
        }
        let mut keep: HashMap<PathBuf, MotionDevice> = std::mem::take(&mut self.devices)
            .into_iter()
            .map(|device| (device.path.clone(), device))
            .collect();
        let vanished: Vec<PathBuf> = keep
            .keys()
            .filter(|path| !known_paths.contains(path))
            .cloned()
            .collect();
        let mut dropped = Vec::new();
        for path in vanished {
            if let Some(device) = keep.remove(&path) {
                dropped.push(device);
            }
        }
        self.devices = discover(&known_paths, keep, self.scale);
        self.known_paths = known_paths;
        self.stale = false;
        dropped
            .iter()
            .flat_map(MotionDevice::stop_readings)
            .collect()
    }
}

impl Default for EvdevSensorBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorBackend for EvdevSensorBackend {
    fn poll_sensors(&mut self) -> Vec<SensorReading> {
        let mut out = Vec::new();
        if self.last_scan.elapsed() >= self.rescan_after {
            out = self.rescan();
        }
        let mut failed = false;
        self.devices.retain_mut(|device| {
            device.drain();
            if device.gone {
                out.extend(device.stop_readings());
                failed = true;
                false
            } else {
                out.extend(device.readings());
                true
            }
        });
        if failed {
            self.stale = true;
        }
        out
    }
}

fn discover(
    paths: &[PathBuf],
    mut keep: HashMap<PathBuf, MotionDevice>,
    scale: MotionScale,
) -> Vec<MotionDevice> {
    let mut devices: Vec<MotionDevice> = Vec::new();
    let mut fresh: Vec<(PathBuf, evdev::Device)> = Vec::new();
    for path in paths {
        if let Some(existing) = keep.remove(path) {
            devices.push(existing);
            continue;
        }
        match evdev::Device::open(path) {
            Ok(device) => match device.set_nonblocking(true) {
                Ok(()) => fresh.push((path.clone(), device)),
                Err(error) => log::warn!("sensor: {}: {error}", path.display()),
            },
            Err(error) => log::debug!("sensor: {}: {error}", path.display()),
        }
    }

    let pads = pad_names(&fresh);
    for (path, device) in fresh {
        if !device.properties().contains(evdev::PropType::ACCELEROMETER) {
            continue;
        }
        let resolutions = abs_state(&device).0;
        let name = resolve_name(&device, &pads);
        let accelerometer = axis_set(
            &device,
            ACCEL_AXES,
            scale.accelerometer,
            "accelerometer",
            &resolutions,
        );
        let gyroscope = axis_set(
            &device,
            GYRO_AXES,
            scale.gyroscope,
            "gyroscope",
            &resolutions,
        );
        if accelerometer.is_none() && gyroscope.is_none() {
            continue;
        }
        log::info!(
            "sensor: {name} ({}) gyro={} accel={}",
            path.display(),
            gyroscope.is_some(),
            accelerometer.is_some()
        );
        devices.push(MotionDevice {
            name,
            frame: frame_for(device.input_id().vendor()),
            path,
            device,
            gyroscope,
            accelerometer,
            gyro_raw: [0; 3],
            accel_raw: [0; 3],
            gyro_dirty: false,
            accel_dirty: false,
            gone: false,
        });
        devices
            .last_mut()
            .expect("motion device just pushed")
            .resync();
    }
    devices
}

/// Pad node names indexed by every key they share with their motion node.
fn pad_names(fresh: &[(PathBuf, evdev::Device)]) -> HashMap<String, Vec<String>> {
    let mut pads: HashMap<String, Vec<String>> = HashMap::new();
    for (_, device) in fresh {
        if !is_pad(device) {
            continue;
        }
        let Some(name) = device.name().filter(|name| !name.is_empty()) else {
            continue;
        };
        for key in correlate_keys(device) {
            pads.entry(key).or_default().push(name.to_string());
        }
    }
    pads
}

/// Keys tying every node of one controller together, most reliable first.
fn correlate_keys(device: &evdev::Device) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(phys) = device.physical_path().filter(|phys| !phys.is_empty()) {
        keys.push(format!("phys:{phys}"));
    }
    if let Some(uniq) = device.unique_name().filter(|uniq| !uniq.is_empty()) {
        keys.push(format!("uniq:{uniq}"));
    }
    let id = device.input_id();
    keys.push(format!("id:{:04x}:{:04x}", id.vendor(), id.product()));
    keys
}

/// The name the gamepad backend reports for this motion node: the sibling
/// pad's name when exactly one matches a key, otherwise the node's own name
/// with its motion suffix removed.
fn resolve_name(device: &evdev::Device, pads: &HashMap<String, Vec<String>>) -> String {
    for key in correlate_keys(device) {
        if let Some([only]) = pads.get(&key).map(Vec::as_slice) {
            return only.clone();
        }
    }
    strip_motion_suffix(device.name().unwrap_or_default()).to_owned()
}

/// Drops the suffix a driver appends to its motion node name.
fn strip_motion_suffix(name: &str) -> &str {
    MOTION_SUFFIXES
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix))
        .unwrap_or(name)
}

/// hid-nintendo reports x, y, z in an order no other driver uses, so permute
/// it back and keep one frame across pads.
fn nintendo_frame(data: [f32; 3]) -> [f32; 3] {
    [-data[1], data[2], -data[0]]
}

fn frame_for(vendor: u16) -> fn([f32; 3]) -> [f32; 3] {
    if vendor == NINTENDO_VENDOR {
        nintendo_frame
    } else {
        std::convert::identity
    }
}

fn is_pad(device: &evdev::Device) -> bool {
    device
        .supported_keys()
        .is_some_and(|keys| GAMEPAD_BUTTONS.iter().any(|code| keys.contains(*code)))
}

/// Reads `codes` off a node that supports them, scaling each axis by its
/// `AbsInfo` resolution (counts per unit). `None` unless the node reports all
/// three: a partial trio would read as a phantom zero on the missing axis,
/// where SDL reports no sensor at all.
fn axis_set(
    device: &evdev::Device,
    codes: [evdev::AbsoluteAxisCode; 3],
    fallback: f32,
    kind: &str,
    resolutions: &HashMap<u16, f32>,
) -> Option<AxisSet> {
    let supported = device.supported_absolute_axes()?;
    if !codes.iter().all(|code| supported.contains(*code)) {
        return None;
    }
    let mut scale = [fallback; 3];
    for (axis, code) in codes.iter().enumerate() {
        match resolutions.get(&code.0) {
            Some(resolution) => scale[axis] = 1.0 / resolution,
            None => log::warn!(
                "sensor: {}: {kind} axis {} has no resolution, assuming {fallback} per count",
                device.name().unwrap_or("?"),
                code.0
            ),
        }
    }
    Some(AxisSet {
        codes: [codes[0].0, codes[1].0, codes[2].0],
        scale,
    })
}

/// Counts per unit (g, degrees per second) and current raw value of each
/// absolute axis.
fn abs_state(device: &evdev::Device) -> (HashMap<u16, f32>, HashMap<u16, i32>) {
    let mut resolutions = HashMap::new();
    let mut values = HashMap::new();
    match device.get_absinfo() {
        Ok(absinfo) => {
            for (code, info) in absinfo {
                if info.resolution() > 0 {
                    resolutions.insert(code.0, info.resolution() as f32);
                }
                values.insert(code.0, info.value());
            }
        }
        Err(error) => log::warn!(
            "sensor: {}: cannot read axis state ({error}), assuming the configured scale",
            device.name().unwrap_or("?")
        ),
    }
    (resolutions, values)
}

/// Current raw value of each axis, zero where the node reports none.
fn current_raw(values: &HashMap<u16, i32>, codes: &[evdev::AbsoluteAxisCode; 3]) -> [i32; 3] {
    [
        values.get(&codes[0].0).copied().unwrap_or(0),
        values.get(&codes[1].0).copied().unwrap_or(0),
        values.get(&codes[2].0).copied().unwrap_or(0),
    ]
}

/// Every evdev node, sorted so consecutive scans compare cheaply.
fn event_paths() -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir("/dev/input") else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("event"))
        })
        .collect();
    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{ACCEL_AXES, current_raw, frame_for, strip_motion_suffix};

    #[test]
    fn current_raw_zero_fills_axes_the_node_does_not_report() {
        let mut values = HashMap::new();
        values.insert(evdev::AbsoluteAxisCode::ABS_X.0, 11);
        values.insert(evdev::AbsoluteAxisCode::ABS_Z.0, 33);
        assert_eq!(current_raw(&values, &ACCEL_AXES), [11, 0, 33]);
        assert_eq!(current_raw(&HashMap::new(), &ACCEL_AXES), [0, 0, 0]);
    }

    #[test]
    fn motion_node_names_resolve_to_their_pad_name() {
        assert_eq!(
            strip_motion_suffix("Steam Deck Motion Sensors"),
            "Steam Deck"
        );
        assert_eq!(
            strip_motion_suffix(
                "Sony Interactive Entertainment Wireless Controller Motion Sensors"
            ),
            "Sony Interactive Entertainment Wireless Controller"
        );
        assert_eq!(
            strip_motion_suffix("Nintendo Switch Pro Controller (IMU)"),
            "Nintendo Switch Pro Controller"
        );
        assert_eq!(strip_motion_suffix("Steam Deck"), "Steam Deck");
    }

    #[test]
    fn only_nintendo_axes_are_rotated_into_the_shared_frame() {
        assert_eq!(frame_for(0x057e)([1.0, 2.0, 3.0]), [-2.0, 3.0, -1.0]);
        assert_eq!(frame_for(0x054c)([1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
        assert_eq!(frame_for(0x028e)([1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
    }
}
