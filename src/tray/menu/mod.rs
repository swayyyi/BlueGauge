pub mod about;
pub mod handler;
pub mod item;
pub mod registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuGroup {
    CheckBoxNotify,
    CheckBoxTrayTooltip,
    RadioDevice,
    RadioLowBattery,
    RadioTrayIconStyle,
}
