use std::path::PathBuf;

use anyhow::{Result, anyhow};

#[derive(Debug, PartialEq)]
enum Device {
    MC0,
    MC1,
    FILE,
}

impl TryFrom<&str> for Device {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Device> {
        match value {
            "mc0" => Ok(Device::MC0),
            "mc1" => Ok(Device::MC1),
            "file" => Ok(Device::FILE),
            _ => Err(anyhow!("unknown device type {value}")),
        }
    }
}

#[derive(Debug, PartialEq)]
struct Path {
    device: Device,
    path: PathBuf,
}

impl TryFrom<&str> for Path {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Path> {
        let parts: Vec<&str> = value.split(":").collect();
        if parts.len() != 2 {
            return Err(anyhow!("wrong number of parts"));
        }

        let device = Device::try_from(parts[0])?;
        let path = PathBuf::from(parts[1]);

        Ok(Path { device, path })
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("mc0:/", Path { device: Device::MC0, path: PathBuf::from("/") })]
    #[case("mc1:/", Path { device: Device::MC1, path: PathBuf::from("/") })]
    #[case("file:/", Path { device: Device::FILE, path: PathBuf::from("/") })]
    fn path_ok(#[case] input: &str, #[case] expected: Path) {
        let actual = Path::try_from(input).unwrap();
        assert_eq!(actual, expected);
    }

    #[rstest]
    #[case("mc2:/")]
    #[case("foo:/")]
    fn path_err(#[case] input: &str) {
        Path::try_from(input).unwrap_err();
    }
}
