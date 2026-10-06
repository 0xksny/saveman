/*
offset  length  description
0       4       PS2D
4       2       Reserved (zero)
6       2       Offset of the second line in the title name
8       4       Reserved (zero)
12      4       Background transparency (0x00 transparent, 0x80 opaque)
16      16      Background color upper left
32      16      Background color upper right
48      16      Background color lower left
64      16      Background color lower right
80      16      Light 1 direction
96      16      Light 2 direction
112     16      Light 3 direction
128     16      Light 1 RGB
144     16      Light 2 RGB
160     16      Light 3 RGB
176     16      Ambient light RGB
192     68      Save title (null terminated, Shift JIS)
260     64      Normal icon filename (null terminated)
324     64      Copy icon filename (null terminated)
388     64      Delete icon filename (null terminated)
452     512     Reserved
*/

use anyhow::{Context, Result};
use encoding_rs::SHIFT_JIS;

const MAGIC_OFFSET: usize = 0;
const MAGIC_LENGTH: usize = 4;
const RESERVED1_OFFSET: usize = 4;
const RESERVED1_LENGTH: usize = 2;
const SECOND_LINE_OFFSET: usize = 6;
const SECOND_LINE_LENGTH: usize = 2;
const RESERVED2_OFFSET: usize = 8;
const RESERVED2_LENGTH: usize = 4;
const TRANSPARENCY_OFFSET: usize = 12;
const TRANSPARENCY_LENGTH: usize = 4;
const BACKGROUND_UPPER_LEFT_OFFSET: usize = 16;
const BACKGROUND_UPPER_RIGHT_OFFSET: usize = 32;
const BACKGROUND_LOWER_LEFT_OFFSET: usize = 48;
const BACKGROUND_LOWER_RIGHT_OFFSET: usize = 64;
const LIGHT1_DIRECTION_OFFSET: usize = 80;
const LIGHT2_DIRECTION_OFFSET: usize = 96;
const LIGHT3_DIRECTION_OFFSET: usize = 112;
const LIGHT1_RGB_OFFSET: usize = 128;
const LIGHT2_RGB_OFFSET: usize = 144;
const LIGHT3_RGB_OFFSET: usize = 160;
const AMBIENT_LIGHT_RGB_OFFSET: usize = 176;
const COLOR_OR_LIGHT_LENGTH: usize = 16;
const TITLE_OFFSET: usize = 192;
const TITLE_LENGTH: usize = 68;
const ICON_NAME_OFFSET: usize = 260;
const ICON_NAME_LENGTH: usize = 64;
const COPY_ICON_NAME_OFFSET: usize = 324;
const COPY_ICON_NAME_LENGTH: usize = 64;
const DELETE_ICON_NAME_OFFSET: usize = 388;
const DELETE_ICON_NAME_LENGTH: usize = 64;
const RESERVED3_OFFSET: usize = 452;
const RESERVED3_LENGTH: usize = 512;

pub struct IconSys {
    bytes: Vec<u8>,
}

impl IconSys {
    pub fn new(bytes: Vec<u8>) -> Self {
        IconSys { bytes }
    }

    fn get_bytes(&self, offset: usize, length: usize) -> Result<Vec<u8>> {
        let end = offset
            .checked_add(length)
            .context("icon.sys range overflow")?;
        Ok(self
            .bytes
            .get(offset..end)
            .context("icon.sys data is too short")?
            .to_vec())
    }

    fn get_u16(&self, offset: usize) -> Result<u16> {
        Ok(u16::from_le_bytes(
            self.bytes
                .get(offset..offset + 2)
                .context("icon.sys data is too short")?
                .try_into()
                .context("reading 16-bit icon.sys value")?,
        ))
    }

    fn get_u32(&self, offset: usize) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.bytes
                .get(offset..offset + 4)
                .context("icon.sys data is too short")?
                .try_into()
                .context("reading 32-bit icon.sys value")?,
        ))
    }

    fn get_null_terminated_bytes(&self, offset: usize, length: usize) -> Result<Vec<u8>> {
        let mut bytes = self.get_bytes(offset, length)?;
        if let Some(end) = bytes.iter().position(|byte| *byte == 0) {
            bytes.truncate(end);
        }
        Ok(bytes)
    }

    pub fn get_magic(&self) -> Result<String> {
        String::from_utf8(self.get_bytes(MAGIC_OFFSET, MAGIC_LENGTH)?)
            .context("creating string from icon.sys magic")
    }

    pub fn get_reserved1(&self) -> Result<u16> {
        self.get_u16(RESERVED1_OFFSET)
    }

    pub fn get_second_line_offset(&self) -> Result<u16> {
        self.get_u16(SECOND_LINE_OFFSET)
    }

    pub fn get_reserved2(&self) -> Result<u32> {
        self.get_u32(RESERVED2_OFFSET)
    }

    pub fn get_transparency(&self) -> Result<u32> {
        self.get_u32(TRANSPARENCY_OFFSET)
    }

    pub fn get_background_color_upper_left(&self) -> Result<Vec<u8>> {
        self.get_bytes(BACKGROUND_UPPER_LEFT_OFFSET, COLOR_OR_LIGHT_LENGTH)
    }

    pub fn get_background_color_upper_right(&self) -> Result<Vec<u8>> {
        self.get_bytes(BACKGROUND_UPPER_RIGHT_OFFSET, COLOR_OR_LIGHT_LENGTH)
    }

    pub fn get_background_color_lower_left(&self) -> Result<Vec<u8>> {
        self.get_bytes(BACKGROUND_LOWER_LEFT_OFFSET, COLOR_OR_LIGHT_LENGTH)
    }

    pub fn get_background_color_lower_right(&self) -> Result<Vec<u8>> {
        self.get_bytes(BACKGROUND_LOWER_RIGHT_OFFSET, COLOR_OR_LIGHT_LENGTH)
    }

    pub fn get_light1_direction(&self) -> Result<Vec<u8>> {
        self.get_bytes(LIGHT1_DIRECTION_OFFSET, COLOR_OR_LIGHT_LENGTH)
    }

    pub fn get_light2_direction(&self) -> Result<Vec<u8>> {
        self.get_bytes(LIGHT2_DIRECTION_OFFSET, COLOR_OR_LIGHT_LENGTH)
    }

    pub fn get_light3_direction(&self) -> Result<Vec<u8>> {
        self.get_bytes(LIGHT3_DIRECTION_OFFSET, COLOR_OR_LIGHT_LENGTH)
    }

    pub fn get_light1_rgb(&self) -> Result<Vec<u8>> {
        self.get_bytes(LIGHT1_RGB_OFFSET, COLOR_OR_LIGHT_LENGTH)
    }

    pub fn get_light2_rgb(&self) -> Result<Vec<u8>> {
        self.get_bytes(LIGHT2_RGB_OFFSET, COLOR_OR_LIGHT_LENGTH)
    }

    pub fn get_light3_rgb(&self) -> Result<Vec<u8>> {
        self.get_bytes(LIGHT3_RGB_OFFSET, COLOR_OR_LIGHT_LENGTH)
    }

    pub fn get_ambient_light_rgb(&self) -> Result<Vec<u8>> {
        self.get_bytes(AMBIENT_LIGHT_RGB_OFFSET, COLOR_OR_LIGHT_LENGTH)
    }

    pub fn get_title(&self) -> Result<String> {
        let title = self
            .get_null_terminated_bytes(TITLE_OFFSET, TITLE_LENGTH)
            .context("getting null terminated bytes")?;

        let (text, _, _) = SHIFT_JIS.decode(title.as_ref());

        Ok(text.into_owned())
    }

    pub fn get_icon_name(&self) -> Result<Vec<u8>> {
        self.get_null_terminated_bytes(ICON_NAME_OFFSET, ICON_NAME_LENGTH)
    }

    pub fn get_copy_icon_name(&self) -> Result<Vec<u8>> {
        self.get_null_terminated_bytes(COPY_ICON_NAME_OFFSET, COPY_ICON_NAME_LENGTH)
    }

    pub fn get_delete_icon_name(&self) -> Result<Vec<u8>> {
        self.get_null_terminated_bytes(DELETE_ICON_NAME_OFFSET, DELETE_ICON_NAME_LENGTH)
    }

    pub fn get_reserved3(&self) -> Result<Vec<u8>> {
        self.get_bytes(RESERVED3_OFFSET, RESERVED3_LENGTH)
    }
}
