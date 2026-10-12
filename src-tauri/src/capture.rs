use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDeviceInfo {
    pub name: String,
    pub id: String,
    pub is_default: bool,
}

pub fn list_input_devices() -> Result<Vec<InputDeviceInfo>, cpal::Error> {
    let host = cpal::default_host();

    let default_device_id = host
        .default_input_device()
        .and_then(|device| device.id().ok());

    host.input_devices()?
        .filter(|device| device.id().is_ok())
        .map(|device| -> Result<InputDeviceInfo, cpal::Error> {
            let device_id = device.id()?;
            let device_description = device.description()?;

            Ok(InputDeviceInfo {
                name: device_description.name().into(),
                id: device_id.to_string(),
                // Don't consider this condition true if both are none
                is_default: default_device_id.is_some() && default_device_id == device.id().ok(),
            })
        })
        .collect()
}
