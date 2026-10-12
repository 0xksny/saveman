use std::str::FromStr;

use anyhow::{Result, anyhow};
use typed_path::{UnixPath, UnixPathBuf};

#[derive(Clone, Debug, PartialEq)]
enum Device {
    MC0,
}

impl FromStr for Device {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "mc0" => Ok(Device::MC0),
            _ => Err(anyhow!("unknown device {value}")),
        }
    }
}

impl TryFrom<&str> for Device {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        value.parse()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    device: Device,
    path: UnixPathBuf,
}

impl Path {
    pub fn get_path(&self) -> &UnixPath {
        &self.path
    }
}

impl FromStr for Path {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Path> {
        let parts: Vec<&str> = value.split(":").collect();
        if parts.len() < 2 {
            return Err(anyhow!("bad format"));
        }

        let device = Device::try_from(parts[0])?;
        let path = UnixPathBuf::try_from(parts[1..].join(":"))?;
        if !path.is_absolute() {
            return Err(anyhow!("path is not absolute"));
        }

        Ok(Path { device, path })
    }
}

impl TryFrom<&str> for Path {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self> {
        value.parse()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case::device_mc0("mc0:/", Path { device: Device::MC0, path: UnixPathBuf::from("/") })]
    #[case::absolute_path("mc0:/foo/bar", Path { device: Device::MC0, path: UnixPathBuf::from("/foo/bar") })]
    fn path_ok(#[case] input: &str, #[case] expected: Path) {
        let actual = Path::try_from(input).unwrap();
        assert_eq!(actual, expected);
    }

    #[rstest]
    #[case::bad_format("foo")]
    #[case::unknown_memory_card_device("mc1:/")]
    #[case::unknown_device("foo:/")]
    #[case::relative_path("mc0:foo/bar")]
    #[case::windows_path("mc0:C:\\")]
    fn path_err(#[case] input: &str) {
        Path::try_from(input).unwrap_err();
    }
}
